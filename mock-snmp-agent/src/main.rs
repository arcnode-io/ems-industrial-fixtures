//! mock-snmp-agent — SNMP v2c UDP server fixture for gateway testing.
//! Serves GetRequest PDUs from a simulated OID value map.

mod oids;
mod simulator;
mod usm;
mod v3;

use rasn::types::ObjectIdentifier;
use rasn_smi::v2::ObjectSyntax;
use rasn_snmp::v2::{Pdu, Pdus, VarBind, VarBindValue};
use rasn_snmp::v2c::Message;
use simulator::Simulator;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tracing::{info, warn};
use v3::AgentState;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_target(false).init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(161);
    let tick_ms: u64 = std::env::var("TICK_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let socket = Arc::new(UdpSocket::bind(format!("0.0.0.0:{port}")).await?);
    info!(%port, tick_ms, "mock-snmp-agent listening");

    let values = Arc::new(Mutex::new(oids::initial_values()));

    // Optional SNMPv3 USM — enabled when SNMP_V3_AUTH_PASS is set.
    // SNMP_V3_USER defaults to "gateway"; matches the gateway's default
    // security_name in `src/snmp/client.rs`.
    let agent_state = std::env::var("SNMP_V3_AUTH_PASS").ok().map(|auth_pass| {
        let priv_pass = std::env::var("SNMP_V3_PRIV_PASS")
            .expect("SNMP_V3_PRIV_PASS required when v3 is enabled");
        let user = std::env::var("SNMP_V3_USER").unwrap_or_else(|_| "gateway".to_string());
        // Deterministic engine id for fixtures so a restart doesn't trigger
        // the client to re-discover. Format=5 (octets), distinct from any
        // legitimate enterprise to keep this clearly a test artifact.
        let engine_id = vec![
            0x80, 0x00, 0x86, 0x9F, 5, b'm', b'o', b'c', b'k', 0, 0, 0, 1,
        ];
        let mut s = AgentState::new(engine_id);
        s.register_user(&user, auth_pass.as_bytes(), priv_pass.as_bytes());
        info!(user, "SNMPv3 USM enabled (authPriv SHA-256 / AES-128)");
        Arc::new(Mutex::new(s))
    });

    // Simulator tick task.
    let sim_values = values.clone();
    tokio::spawn(async move {
        let sim = Simulator::new();
        loop {
            {
                let mut v = sim_values.lock().await;
                sim.tick(&mut v);
            }
            tokio::time::sleep(Duration::from_millis(tick_ms)).await;
        }
    });

    let mut buf = vec![0u8; 65536];
    loop {
        let (n, peer) = socket.recv_from(&mut buf).await?;
        let datagram = buf[..n].to_vec();
        let socket = socket.clone();
        let values = values.clone();
        let agent_state = agent_state.clone();
        tokio::spawn(async move {
            // Branch by SNMP version peeked from the BER header.
            let reply = if peek_snmp_version(&datagram) == Some(3) {
                match agent_state {
                    Some(s) => v3::handle_v3_datagram(&datagram, &s, &values).await,
                    None => {
                        warn!("v3 message received but SNMP_V3_AUTH_PASS not set");
                        None
                    }
                }
            } else {
                match handle_datagram(&datagram, &values).await {
                    Ok(b) => Some(b),
                    Err(e) => {
                        warn!(error = %e, "v2c handle_datagram failed");
                        None
                    }
                }
            };
            if let Some(reply) = reply
                && let Err(e) = socket.send_to(&reply, peer).await
            {
                warn!(error = %e, "send_to failed");
            }
        });
    }
}

/// Peek the SNMP version byte from a BER-encoded message. Returns None on
/// malformed input. Used to dispatch v2c vs v3 before full decode.
fn peek_snmp_version(datagram: &[u8]) -> Option<u8> {
    if datagram.first() != Some(&0x30) {
        return None;
    }
    let len_byte = *datagram.get(1)?;
    let header_len = if len_byte < 0x80 {
        2
    } else {
        2 + usize::from(len_byte & 0x7F)
    };
    if datagram.get(header_len) != Some(&0x02) || datagram.get(header_len + 1) != Some(&0x01) {
        return None;
    }
    datagram.get(header_len + 2).copied()
}

/// Decode a raw datagram, build a Response, encode it back to bytes.
async fn handle_datagram(
    datagram: &[u8],
    values: &Mutex<HashMap<Vec<u32>, i64>>,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let message: Message<Pdus> = rasn::ber::decode(datagram)?;

    let response_pdu = match message.data {
        Pdus::GetRequest(req) => build_response(&req.0, values, false).await,
        Pdus::GetNextRequest(req) => build_response(&req.0, values, true).await,
        _ => {
            return Err("unsupported PDU type (Tier 1: GetRequest + GetNextRequest only)".into());
        }
    };

    let response = Message {
        version: message.version,
        community: message.community,
        data: Pdus::Response(rasn_snmp::v2::Response(response_pdu)),
    };
    Ok(rasn::ber::encode(&response)?)
}

/// Build a Response PDU. For GetRequest: respond with the exact OID's value.
/// For GetNextRequest: find the lexicographic-next OID in the map.
///
/// `pub(crate)` so the v3 module can reuse this for the scoped-PDU payload.
pub(crate) async fn build_response(
    request: &Pdu,
    values: &Mutex<HashMap<Vec<u32>, i64>>,
    is_get_next: bool,
) -> Pdu {
    let map = values.lock().await;
    let mut response_vars: Vec<VarBind> = Vec::with_capacity(request.variable_bindings.len());

    for vb in &request.variable_bindings {
        let req_oid: Vec<u32> = vb.name.iter().copied().collect();
        let (resp_oid, resp_value) = if is_get_next {
            let mut candidates: Vec<&Vec<u32>> = map.keys().filter(|k| **k > req_oid).collect();
            candidates.sort();
            match candidates.first() {
                Some(next) => ((*next).clone(), map.get(*next).copied()),
                None => (req_oid.clone(), None),
            }
        } else {
            (req_oid.clone(), map.get(&req_oid).copied())
        };

        let oid = ObjectIdentifier::new(resp_oid).expect("non-empty OID");
        let value = match resp_value {
            Some(v) => VarBindValue::Value(ObjectSyntax::from(v as i32)),
            // Miss-case semantics per RFC 3416:
            //   GetNextRequest: no lexicographic successor → EndOfMibView
            //   GetRequest:     OID has no instance        → NoSuchInstance
            None if is_get_next => VarBindValue::EndOfMibView,
            None => VarBindValue::NoSuchInstance,
        };
        response_vars.push(VarBind { name: oid, value });
    }

    Pdu {
        request_id: request.request_id,
        error_status: 0,
        error_index: 0,
        variable_bindings: response_vars,
    }
}
