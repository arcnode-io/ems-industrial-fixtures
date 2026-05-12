//! BACnet/IP responder. Listens on UDP 47808, decodes BVLC + NPDU + APDU,
//! answers `Who-Is` with `I-Am` and `ReadProperty` of `present_value` on any
//! tracked `AnalogInput` object.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use bacnet_rs::app::Apdu;
use bacnet_rs::network::Npdu;
use bacnet_rs::object::{ObjectIdentifier, ObjectType, PropertyIdentifier, Segmentation};
use bacnet_rs::property::PropertyValue;
use bacnet_rs::service::{
    ConfirmedServiceChoice, IAmRequest, ReadPropertyRequest, ReadPropertyResponse,
    UnconfirmedServiceChoice, WhoIsRequest,
};
use mock_bacnet_device::simulator::{SawStrategy, Values, seed, tick};
use mock_bacnet_device::{DEFAULT_DEVICE_INSTANCE, DEFAULT_PORT};
use tokio::net::UdpSocket;
use tracing::{info, warn};

/// Simulated objects: instance → (sawtooth, units).
fn strategies() -> HashMap<u32, SawStrategy> {
    let mut m = HashMap::new();
    // AI #1: supply water temp, 7..15 °C step 0.5 — matches dry-cooler outlet.
    m.insert(
        1,
        SawStrategy {
            min: 7.0,
            max: 15.0,
            step: 0.5,
        },
    );
    m
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let strategies = strategies();
    let values: Values = seed(&strategies);

    // Background tick.
    let tick_values = values.clone();
    let tick_strategies = strategies.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            tick(&tick_values, &tick_strategies).await;
        }
    });

    let bind = format!("0.0.0.0:{DEFAULT_PORT}");
    let socket = UdpSocket::bind(&bind).await.context("bind 47808")?;
    info!("mock-bacnet-device listening on {bind} (device {DEFAULT_DEVICE_INSTANCE})");

    let mut buf = [0u8; 1500];
    loop {
        let (n, peer) = socket.recv_from(&mut buf).await?;
        let frame = &buf[..n];
        match handle(frame, &values).await {
            Ok(Some(reply)) => {
                let _ = socket.send_to(&reply, peer).await;
            }
            Ok(None) => {}
            Err(e) => warn!(error = %e, "frame handler failed"),
        }
    }
}

/// Dispatch on the inbound frame; returns the reply bytes if any.
async fn handle(frame: &[u8], values: &Values) -> Result<Option<Vec<u8>>> {
    // BVLC: must be 0x81 (BACnet/IP), 0x0A (Original-Unicast-NPDU) or 0x0B (Broadcast).
    if frame.len() < 4 || frame[0] != 0x81 {
        return Ok(None);
    }
    let function = frame[1];
    if function != 0x0A && function != 0x0B {
        return Ok(None);
    }
    let (_npdu, npdu_len) =
        Npdu::decode(&frame[4..]).map_err(|e| anyhow::anyhow!("npdu decode: {e:?}"))?;
    let apdu_bytes = &frame[4 + npdu_len..];
    let apdu = Apdu::decode(apdu_bytes).map_err(|e| anyhow::anyhow!("apdu decode: {e:?}"))?;

    match apdu {
        Apdu::UnconfirmedRequest {
            service_choice: UnconfirmedServiceChoice::WhoIs,
            service_data,
        } => {
            let req = WhoIsRequest::decode(&service_data)
                .map_err(|e| anyhow::anyhow!("who-is decode: {e:?}"))?;
            if !req.matches(DEFAULT_DEVICE_INSTANCE) {
                return Ok(None);
            }
            Ok(Some(iam_frame()?))
        }
        Apdu::ConfirmedRequest {
            invoke_id,
            service_choice: ConfirmedServiceChoice::ReadProperty,
            service_data,
            ..
        } => {
            let req = ReadPropertyRequest::decode(&service_data)
                .map_err(|e| anyhow::anyhow!("rp decode: {e:?}"))?;
            answer_read_property(invoke_id, &req, values).await
        }
        _ => Ok(None),
    }
}

/// Build the `I-Am` reply broadcast frame.
fn iam_frame() -> Result<Vec<u8>> {
    let device_id = ObjectIdentifier::new(ObjectType::Device, DEFAULT_DEVICE_INSTANCE);
    let iam = IAmRequest::new(device_id, 1476, Segmentation::NoSegmentation, 260);
    let mut svc = Vec::new();
    iam.encode(&mut svc)
        .map_err(|e| anyhow::anyhow!("iam encode: {e:?}"))?;
    let apdu = Apdu::UnconfirmedRequest {
        service_choice: UnconfirmedServiceChoice::IAm,
        service_data: svc,
    };
    Ok(wrap_bvlc(apdu.encode(), 0x0B)) // broadcast
}

/// Build a `ComplexAck` carrying the current `present_value` for the request's
/// AI instance. Replies with `Reject` (not implemented here — drop the frame)
/// if the object/property isn't known.
async fn answer_read_property(
    invoke_id: u8,
    req: &ReadPropertyRequest,
    values: &Values,
) -> Result<Option<Vec<u8>>> {
    if req.object_identifier.object_type != ObjectType::AnalogInput
        || req.property_identifier != PropertyIdentifier::PresentValue
    {
        return Ok(None);
    }
    let guard = values.lock().await;
    let Some(&v) = guard.get(&req.object_identifier.instance) else {
        return Ok(None);
    };
    drop(guard);
    let resp = ReadPropertyResponse::new(
        req.object_identifier,
        req.property_identifier,
        vec![PropertyValue::Real(v)],
    );
    let mut svc = Vec::new();
    resp.encode(&mut svc)
        .map_err(|e| anyhow::anyhow!("rp resp encode: {e:?}"))?;
    let apdu = Apdu::ComplexAck {
        segmented: false,
        more_follows: false,
        invoke_id,
        sequence_number: None,
        proposed_window_size: None,
        service_choice: ConfirmedServiceChoice::ReadProperty,
        service_data: svc,
    };
    Ok(Some(wrap_bvlc(apdu.encode(), 0x0A))) // unicast
}

/// Wrap an APDU in NPDU + BVLC. `function` is `0x0A` for unicast or `0x0B`
/// for broadcast per Annex J.
fn wrap_bvlc(apdu: Vec<u8>, function: u8) -> Vec<u8> {
    let mut npdu = Npdu::new();
    npdu.control.expecting_reply = false;
    let npdu_bytes = npdu.encode();
    let mut payload = npdu_bytes;
    payload.extend_from_slice(&apdu);
    let mut out = vec![0x81, function, 0x00, 0x00];
    out.extend_from_slice(&payload);
    let total = out.len() as u16;
    out[2] = (total >> 8) as u8;
    out[3] = (total & 0xFF) as u8;
    out
}
