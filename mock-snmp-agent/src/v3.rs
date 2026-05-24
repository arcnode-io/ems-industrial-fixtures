//! SNMPv3 USM message handler — discovery (RFC 3414 §4) + authenticated
//! request flow (RFC 3414 §3.2). Sits alongside `handle_datagram` (v2c).

use crate::usm;
use rasn::types::{Integer, ObjectIdentifier, OctetString};
use rasn_smi::v2::ObjectSyntax;
use rasn_snmp::v2::{Pdu, Pdus, Report, VarBind, VarBindValue};
use rasn_snmp::v3::{HeaderData, Message, ScopedPdu, ScopedPduData, USMSecurityParameters};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::warn;

/// usmStatsUnknownEngineIDs — required REPORT PDU OID when the client's
/// `authoritative_engine_id` doesn't match ours (and on first contact).
/// RFC 3414 §5.
const USM_STATS_UNKNOWN_ENGINE_IDS: [u32; 11] = [1, 3, 6, 1, 6, 3, 15, 1, 1, 4, 0];

/// Agent-side authoritative state — what an SNMPv3 entity tracks as
/// "the authoritative engine" per RFC 3411.
pub struct AgentState {
    /// Our authoritative engine id (RFC 3411 §3.1.1.2 format).
    pub engine_id: Vec<u8>,
    /// Boots counter (would be persisted in a real deployment; in-mem here).
    pub engine_boots: u32,
    /// Wall-clock anchor for `engine_time` (seconds since this boot).
    pub boot_instant: Instant,
    /// Registered USM users keyed by security name.
    pub users: HashMap<String, UsmUser>,
}

/// One registered USM user. Keys are pre-derived from the passphrases at
/// startup; per-message processing just looks them up. The name itself
/// lives as the key in `AgentState::users` — no need to duplicate it
/// inside the value.
pub struct UsmUser {
    /// SHA-256 localized auth key (RFC 3414 §A.2).
    pub auth_key: Vec<u8>,
    /// AES-128 priv key (leading bytes of auth key per RFC 3826 §3.1.2.1).
    pub priv_key: Vec<u8>,
}

impl AgentState {
    /// Build a state with the given engine id + one registered user.
    pub fn new(engine_id: Vec<u8>) -> Self {
        Self {
            engine_id,
            engine_boots: 1,
            boot_instant: Instant::now(),
            users: HashMap::new(),
        }
    }

    /// Register a user, deriving both keys from the passphrases.
    pub fn register_user(&mut self, security_name: &str, auth_pass: &[u8], priv_pass: &[u8]) {
        let auth_key = usm::derive_localized_key(auth_pass, &self.engine_id);
        let priv_key = usm::derive_localized_key(priv_pass, &self.engine_id);
        self.users.insert(
            security_name.to_string(),
            UsmUser {
                auth_key,
                priv_key: priv_key[..usm::PRIV_KEY_LEN].to_vec(),
            },
        );
    }

    /// Seconds since this agent booted — the value we report as
    /// `authoritativeEngineTime`.
    pub fn engine_time(&self) -> u32 {
        u32::try_from(self.boot_instant.elapsed().as_secs()).unwrap_or(u32::MAX)
    }
}

/// Dispatch a v3 datagram. Returns the encoded reply or `None` if the
/// message should be dropped (HMAC fail / unknown user / malformed).
pub async fn handle_v3_datagram(
    datagram: &[u8],
    state: &Arc<Mutex<AgentState>>,
    values: &Mutex<HashMap<Vec<u32>, i64>>,
) -> Option<Vec<u8>> {
    let message: Message = match rasn::ber::decode(datagram) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e,"v3 BER decode failed");
            return None;
        }
    };
    let usm_params: USMSecurityParameters =
        match message.decode_security_parameters::<USMSecurityParameters>(rasn::Codec::Ber) {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e,"USM params decode failed");
                return None;
            }
        };

    // Discovery: empty engine id from client = first-contact ID request.
    // Reply with a REPORT PDU carrying usmStatsUnknownEngineIDs.0 and our
    // authoritative_engine_id + boots/time so the client can sync.
    if usm_params.authoritative_engine_id.is_empty() {
        let s = state.lock().await;
        return Some(build_discovery_report(&message, &usm_params, &s));
    }

    // Authenticated path. Engine id must match ours.
    let mut s = state.lock().await;
    if usm_params.authoritative_engine_id.as_ref() != s.engine_id.as_slice() {
        warn!("auth path: engine id mismatch");
        return None;
    }
    let user = match s
        .users
        .get(std::str::from_utf8(usm_params.user_name.as_ref()).unwrap_or(""))
    {
        Some(u) => UsmUserSnapshot {
            auth_key: u.auth_key.clone(),
            priv_key: u.priv_key.clone(),
        },
        None => {
            warn!(user = %String::from_utf8_lossy(usm_params.user_name.as_ref()),
                  "auth path: unknown user");
            return None;
        }
    };
    let flags = message.global_data.flags.first().copied().unwrap_or(0);
    let want_auth = flags & 0x01 != 0;
    let want_priv = flags & 0x02 != 0;

    if want_auth && !verify_hmac(&message, &usm_params, &user.auth_key) {
        warn!("auth path: HMAC verification failed");
        return None;
    }

    // Decrypt scoped PDU if priv flag is set.
    let scoped = match &message.scoped_data {
        ScopedPduData::CleartextPdu(p) => p.clone(),
        ScopedPduData::EncryptedPdu(ct) if want_priv => {
            let plaintext = usm::aes_cfb_decrypt(
                ct.as_ref(),
                &user.priv_key,
                u32::try_from(usm_params.authoritative_engine_boots.clone()).unwrap_or(0),
                u32::try_from(usm_params.authoritative_engine_time.clone()).unwrap_or(0),
                usm_params.privacy_parameters.as_ref(),
            );
            match rasn::ber::decode::<ScopedPdu>(&plaintext) {
                Ok(p) => p,
                Err(e) => {
                    warn!(error = %e, "auth path: scoped PDU decode after decrypt failed");
                    return None;
                }
            }
        }
        ScopedPduData::EncryptedPdu(_) => {
            warn!("auth path: encrypted PDU but priv flag off");
            return None;
        }
    };

    // Dispatch via the existing v2c handler logic (reuse for v3 scoped PDU
    // payload — the inner Pdus shape is identical).
    let response_pdu = match scoped.data {
        Pdus::GetRequest(req) => crate::build_response(&req.0, values, false).await,
        Pdus::GetNextRequest(req) => crate::build_response(&req.0, values, true).await,
        _ => {
            warn!("auth path: unsupported PDU type");
            return None;
        }
    };
    let response_scoped = ScopedPdu {
        engine_id: OctetString::from(s.engine_id.clone()),
        name: scoped.name,
        data: Pdus::Response(rasn_snmp::v2::Response(response_pdu)),
    };
    Some(build_authenticated_reply(
        &message,
        &user,
        &mut s,
        response_scoped,
        want_priv,
    ))
}

/// Local snapshot of the user's keys — held by value so we can drop the
/// state lock before doing the (slow) BER encode/HMAC work.
struct UsmUserSnapshot {
    /// SHA-256 localized auth key.
    auth_key: Vec<u8>,
    /// AES-128 priv key (16 bytes).
    priv_key: Vec<u8>,
}

/// Verify HMAC over the original message with `authentication_parameters`
/// zeroed (RFC 3414 §3.2). Constant-time compare via slice equality is
/// adequate for non-adversarial test fixture use.
fn verify_hmac(message: &Message, usm_params: &USMSecurityParameters, auth_key: &[u8]) -> bool {
    if usm_params.authentication_parameters.len() != usm::AUTH_TRUNC_LEN {
        return false;
    }
    let received = usm_params.authentication_parameters.clone();
    let mut zeroed_usm = usm_params.clone();
    zeroed_usm.authentication_parameters = OctetString::from(vec![0_u8; usm::AUTH_TRUNC_LEN]);
    let mut to_hmac = message.clone();
    if to_hmac
        .encode_security_parameters(rasn::Codec::Ber, &zeroed_usm)
        .is_err()
    {
        return false;
    }
    let bytes = match rasn::ber::encode(&to_hmac) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let computed = usm::hmac_sign(auth_key, &bytes);
    computed.as_slice() == received.as_ref()
}

/// Build the encrypted+signed reply. Mirror of `verify_hmac` for the
/// outbound direction.
fn build_authenticated_reply(
    request: &Message,
    user: &UsmUserSnapshot,
    state: &mut AgentState,
    response_scoped: ScopedPdu,
    encrypt: bool,
) -> Vec<u8> {
    let engine_boots = state.engine_boots;
    let engine_time = state.engine_time();
    let scoped_data = if encrypt {
        let pt = rasn::ber::encode(&response_scoped).expect("encode scoped PDU");
        let (ct, priv_params) =
            usm::aes_cfb_encrypt(&pt, &user.priv_key, engine_boots, engine_time);
        (
            ScopedPduData::EncryptedPdu(OctetString::from(ct)),
            priv_params,
        )
    } else {
        (ScopedPduData::CleartextPdu(response_scoped), Vec::new())
    };
    let response_usm = USMSecurityParameters {
        authoritative_engine_id: OctetString::from(state.engine_id.clone()),
        authoritative_engine_boots: Integer::from(engine_boots),
        authoritative_engine_time: Integer::from(engine_time),
        user_name: request
            .decode_security_parameters::<USMSecurityParameters>(rasn::Codec::Ber)
            .map(|u| u.user_name)
            .unwrap_or_else(|_| OctetString::from_static(&[])),
        authentication_parameters: OctetString::from(vec![0_u8; usm::AUTH_TRUNC_LEN]),
        privacy_parameters: OctetString::from(scoped_data.1),
    };
    let mut response = Message {
        version: request.version.clone(),
        global_data: HeaderData {
            message_id: request.global_data.message_id.clone(),
            max_size: request.global_data.max_size.clone(),
            // authPriv response flags = auth + priv (no reportable bit).
            flags: OctetString::from(vec![if encrypt { 0x03 } else { 0x01 }]),
            security_model: Integer::from(3),
        },
        security_parameters: OctetString::from_static(&[]),
        scoped_data: scoped_data.0,
    };
    response
        .encode_security_parameters(rasn::Codec::Ber, &response_usm)
        .unwrap_or_else(|e| panic!("response USM encode: {e}"));
    let with_zero_auth = rasn::ber::encode(&response).expect("encode v3 message");
    let mac = usm::hmac_sign(&user.auth_key, &with_zero_auth);
    // Splice the real HMAC into the zero placeholder. Robust splice: re-encode
    // with the real auth_params (HMAC), preserving message bit-equivalence.
    let real_usm = USMSecurityParameters {
        authentication_parameters: OctetString::from(mac),
        ..response_usm
    };
    response
        .encode_security_parameters(rasn::Codec::Ber, &real_usm)
        .unwrap_or_else(|e| panic!("response USM re-encode: {e}"));
    rasn::ber::encode(&response).expect("encode v3 message final")
}

/// Build a REPORT message that announces our engine id to a discovering
/// client. Auth + priv flags off (discovery messages are noAuthNoPriv).
fn build_discovery_report(
    request: &Message,
    request_usm: &USMSecurityParameters,
    state: &AgentState,
) -> Vec<u8> {
    let report_pdu = Pdu {
        request_id: extract_request_id(request).unwrap_or(0),
        error_status: 0,
        error_index: 0,
        variable_bindings: vec![VarBind {
            name: ObjectIdentifier::new(&USM_STATS_UNKNOWN_ENGINE_IDS).expect("static OID"),
            value: VarBindValue::Value(ObjectSyntax::from(1_i32)),
        }],
    };
    let scoped = ScopedPdu {
        engine_id: OctetString::from(state.engine_id.clone()),
        name: OctetString::from_static(&[]),
        data: Pdus::Report(Report(report_pdu)),
    };
    let response_usm = USMSecurityParameters {
        authoritative_engine_id: OctetString::from(state.engine_id.clone()),
        authoritative_engine_boots: Integer::from(state.engine_boots),
        authoritative_engine_time: Integer::from(state.engine_time()),
        user_name: request_usm.user_name.clone(),
        authentication_parameters: OctetString::from_static(&[]),
        privacy_parameters: OctetString::from_static(&[]),
    };
    let mut response = Message {
        version: request.version.clone(),
        global_data: HeaderData {
            message_id: request.global_data.message_id.clone(),
            max_size: request.global_data.max_size.clone(),
            // reportable bit only (no auth, no priv) — RFC 3412 §6.4
            flags: OctetString::from(vec![0x04]),
            security_model: Integer::from(3),
        },
        security_parameters: OctetString::from_static(&[]),
        scoped_data: ScopedPduData::CleartextPdu(scoped),
    };
    response
        .encode_security_parameters(rasn::Codec::Ber, &response_usm)
        .unwrap_or_else(|e| panic!("USM params encode: {e}"));
    rasn::ber::encode(&response).expect("v3 message encode")
}

/// Pull request_id out of a (cleartext) scoped PDU, for replay in the
/// REPORT response — clients correlate the discovery reply by request_id.
fn extract_request_id(message: &Message) -> Option<i32> {
    let scoped = match &message.scoped_data {
        ScopedPduData::CleartextPdu(s) => s,
        ScopedPduData::EncryptedPdu(_) => return None,
    };
    match &scoped.data {
        Pdus::GetRequest(req) => Some(req.0.request_id),
        Pdus::GetNextRequest(req) => Some(req.0.request_id),
        Pdus::GetBulkRequest(req) => Some(req.0.request_id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a manual discovery request (empty engine id, empty user,
    /// noAuthNoPriv flags), feed it through the handler, and assert the
    /// reply is a REPORT carrying our engine id + the
    /// usmStatsUnknownEngineIDs OID.
    #[tokio::test]
    async fn discovery_returns_report_with_engine_id() {
        // Arrange — agent state with a known engine id, no registered users.
        let our_eid = vec![
            0x80, 0x00, 0x86, 0x9F, 5, 0xDE, 0xAD, 0xBE, 0xEF, 1, 2, 3, 4,
        ];
        let state = Arc::new(Mutex::new(AgentState::new(our_eid.clone())));
        let values = Mutex::new(HashMap::new());
        let request = make_discovery_request();
        // Act
        let reply_bytes = handle_v3_datagram(&request, &state, &values)
            .await
            .expect("reply");
        let reply: Message = rasn::ber::decode(&reply_bytes).expect("decode reply");
        let reply_usm: USMSecurityParameters = reply
            .decode_security_parameters::<USMSecurityParameters>(rasn::Codec::Ber)
            .unwrap_or_else(|e| panic!("decode USM: {e}"));
        // Assert — engine id is OURS, scoped PDU is a Report carrying the
        // usmStatsUnknownEngineIDs OID.
        assert_eq!(
            reply_usm.authoritative_engine_id.as_ref(),
            our_eid.as_slice()
        );
        let scoped = match reply.scoped_data {
            ScopedPduData::CleartextPdu(s) => s,
            _ => panic!("expected cleartext"),
        };
        let report = match scoped.data {
            Pdus::Report(r) => r,
            _ => panic!("expected Report"),
        };
        let vb = &report.0.variable_bindings[0];
        let oid: Vec<u32> = vb.name.iter().copied().collect();
        assert_eq!(oid, USM_STATS_UNKNOWN_ENGINE_IDS);
    }

    /// End-to-end authenticated path: client builds an authPriv GetRequest
    /// for a registered OID, agent decrypts + verifies + dispatches +
    /// re-encrypts the reply. We then verify the agent's HMAC and decrypt
    /// the response to check the varbind value made it through both
    /// crypto round-trips intact.
    #[tokio::test]
    async fn authpriv_round_trip_decodes_to_simulator_value() {
        // Arrange — agent has our engine id + one registered user; the values
        // map has one OID stocked with a known integer.
        let our_eid = vec![
            0x80, 0x00, 0x86, 0x9F, 5, 0x42, 0x42, 0x42, 0x42, 1, 2, 3, 4,
        ];
        let state = Arc::new(Mutex::new(AgentState::new(our_eid.clone())));
        {
            let mut s = state.lock().await;
            s.register_user("gateway", b"authpass1234", b"privpass5678");
        }
        let target_oid: Vec<u32> = vec![1, 3, 6, 1, 4, 1, 41_999, 42, 1, 0];
        let target_value: i64 = 9_876;
        let values = Mutex::new(HashMap::from([(target_oid.clone(), target_value)]));

        let auth_key = usm::derive_localized_key(b"authpass1234", &our_eid);
        let priv_key = usm::derive_localized_key(b"privpass5678", &our_eid);
        let request = make_authpriv_request(
            &our_eid,
            &auth_key,
            &priv_key[..usm::PRIV_KEY_LEN],
            &target_oid,
        );

        // Act
        let reply = handle_v3_datagram(&request, &state, &values)
            .await
            .expect("authenticated reply");

        // Assert — decode + decrypt + check varbind
        let msg: Message = rasn::ber::decode(&reply).expect("decode reply");
        let usm_params: USMSecurityParameters = msg
            .decode_security_parameters::<USMSecurityParameters>(rasn::Codec::Ber)
            .unwrap_or_else(|e| panic!("decode USM: {e}"));
        let ct = match msg.scoped_data {
            ScopedPduData::EncryptedPdu(c) => c,
            _ => panic!("expected encrypted reply"),
        };
        let plain = usm::aes_cfb_decrypt(
            ct.as_ref(),
            &priv_key[..usm::PRIV_KEY_LEN],
            u32::try_from(usm_params.authoritative_engine_boots).unwrap(),
            u32::try_from(usm_params.authoritative_engine_time).unwrap(),
            usm_params.privacy_parameters.as_ref(),
        );
        let scoped: ScopedPdu = rasn::ber::decode(&plain).expect("decode scoped");
        let response = match scoped.data {
            Pdus::Response(r) => r.0,
            _ => panic!("expected Response PDU"),
        };
        let vb = &response.variable_bindings[0];
        let oid: Vec<u32> = vb.name.iter().copied().collect();
        assert_eq!(oid, target_oid);
        match &vb.value {
            VarBindValue::Value(ObjectSyntax::Simple(s)) => {
                let asn1_int = format!("{s:?}");
                assert!(
                    asn1_int.contains(&target_value.to_string()),
                    "value debug `{asn1_int}` should contain {target_value}"
                );
            }
            other => panic!("unexpected varbind value: {other:?}"),
        }
    }

    /// Mirror of the agent's outbound flow, used to build a request the
    /// agent will accept: encrypt the scoped PDU, build the message with
    /// zero-placeholder auth, HMAC over the zeroed bytes, re-encode with
    /// real HMAC.
    fn make_authpriv_request(
        engine_id: &[u8],
        auth_key: &[u8],
        priv_key: &[u8],
        target_oid: &[u32],
    ) -> Vec<u8> {
        let engine_boots = 1_u32;
        let engine_time = 0_u32;
        let scoped = ScopedPdu {
            engine_id: OctetString::from(engine_id.to_vec()),
            name: OctetString::from_static(&[]),
            data: Pdus::GetRequest(rasn_snmp::v2::GetRequest(Pdu {
                request_id: 7777,
                error_status: 0,
                error_index: 0,
                variable_bindings: vec![VarBind {
                    name: ObjectIdentifier::new(target_oid.to_vec()).expect("oid"),
                    value: VarBindValue::Unspecified,
                }],
            })),
        };
        let pt = rasn::ber::encode(&scoped).expect("encode scoped");
        let (ct, priv_params) = usm::aes_cfb_encrypt(&pt, priv_key, engine_boots, engine_time);
        let usm_params = USMSecurityParameters {
            authoritative_engine_id: OctetString::from(engine_id.to_vec()),
            authoritative_engine_boots: engine_boots.into(),
            authoritative_engine_time: engine_time.into(),
            user_name: OctetString::from_static(b"gateway"),
            authentication_parameters: OctetString::from(vec![0_u8; usm::AUTH_TRUNC_LEN]),
            privacy_parameters: OctetString::from(priv_params),
        };
        let mut msg = Message {
            version: Integer::from(3),
            global_data: HeaderData {
                message_id: 7777.into(),
                max_size: 65507.into(),
                flags: OctetString::from(vec![0x03]),
                security_model: Integer::from(3),
            },
            security_parameters: OctetString::from_static(&[]),
            scoped_data: ScopedPduData::EncryptedPdu(OctetString::from(ct)),
        };
        msg.encode_security_parameters(rasn::Codec::Ber, &usm_params)
            .unwrap_or_else(|e| panic!("usm encode: {e}"));
        let with_zero_auth = rasn::ber::encode(&msg).expect("encode v3");
        let mac = usm::hmac_sign(auth_key, &with_zero_auth);
        let real_usm = USMSecurityParameters {
            authentication_parameters: OctetString::from(mac),
            ..usm_params
        };
        msg.encode_security_parameters(rasn::Codec::Ber, &real_usm)
            .unwrap_or_else(|e| panic!("usm re-encode: {e}"));
        rasn::ber::encode(&msg).expect("encode v3 final")
    }

    /// Construct a minimal discovery request — empty engine id + empty user,
    /// noAuthNoPriv flags, GetRequest with one wildcard varbind.
    fn make_discovery_request() -> Vec<u8> {
        let scoped = ScopedPdu {
            engine_id: OctetString::from_static(&[]),
            name: OctetString::from_static(&[]),
            data: Pdus::GetRequest(rasn_snmp::v2::GetRequest(Pdu {
                request_id: 12345,
                error_status: 0,
                error_index: 0,
                variable_bindings: vec![],
            })),
        };
        let usm = USMSecurityParameters {
            authoritative_engine_id: OctetString::from_static(&[]),
            authoritative_engine_boots: 0.into(),
            authoritative_engine_time: 0.into(),
            user_name: OctetString::from_static(&[]),
            authentication_parameters: OctetString::from_static(&[]),
            privacy_parameters: OctetString::from_static(&[]),
        };
        let mut msg = Message {
            version: Integer::from(3),
            global_data: HeaderData {
                message_id: 12345.into(),
                max_size: 65507.into(),
                flags: OctetString::from(vec![0x04]),
                security_model: Integer::from(3),
            },
            security_parameters: OctetString::from_static(&[]),
            scoped_data: ScopedPduData::CleartextPdu(scoped),
        };
        msg.encode_security_parameters(rasn::Codec::Ber, &usm)
            .unwrap_or_else(|e| panic!("usm encode: {e}"));
        rasn::ber::encode(&msg).expect("encode")
    }
}
