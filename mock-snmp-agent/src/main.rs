//! mock-snmp-agent — SNMP v2c UDP server fixture for gateway testing.
//! Serves GetRequest PDUs from a simulated OID value map.

mod oids;
mod simulator;
mod usm;

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
        tokio::spawn(async move {
            match handle_datagram(&datagram, &values).await {
                Ok(reply) => {
                    if let Err(e) = socket.send_to(&reply, peer).await {
                        warn!(error = %e, "send_to failed");
                    }
                }
                Err(e) => warn!(error = %e, "handle_datagram failed"),
            }
        });
    }
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
async fn build_response(
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
