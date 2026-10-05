//! mock-modbus-server — Modbus TCP / Modbus Security (TLS+Role) server fixture.
//! Reads holding registers updated on a tick by the `Simulator`.
//!
//! Modes selected by env:
//! - `MODBUS_TLS=1` → Modbus Security (mTLS + CA + Role extension authz).
//!   Requires `MODBUS_TLS_CA`, `MODBUS_TLS_CERT`, `MODBUS_TLS_KEY` paths.
//!   Default port: 802 (per Modbus Security spec).
//! - else → plain Modbus/TCP. Default port: 502.
//! - `MODBUS_WRITABLE=1` → accepts function-code-16 writes (independent of
//!   the TLS toggle above). Default: read-only, writes rejected with
//!   `IllegalFunction` — real devices are a mix of read-only meters and
//!   writable setpoints, so this is opt-in per fixture instance.
//! - `MODBUS_PROFILE` → which device the registers model. `poi_meter`
//!   (default); `bess_rack`, a simulated battery rack that follows its
//!   commanded setpoint and drains/fills SoC (see `battery::from_env`); or
//!   `dc_external`, a static dry cooler (see `dc_external`); or
//!   `pv_inverter`, a static SunSpec model 103 inverter (see `pv_inverter`).
//!   `bess_rack` and `dc_external` are always writable.

mod battery;
mod compute_load;
mod control;
mod dc_external;
mod derate;
mod handler;
mod poi;
mod pv_inverter;
mod rack_registers;
mod registers;
mod simulator;
mod tls;

use handler::MeterHandler;
use rodbus::server::{AddressFilter, RequestHandler, ServerHandlerMap, spawn_tcp_server_task};
use rodbus::{DecodeLevel, UnitId};
use simulator::Simulator;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::info;

/// Standards-defined Modbus Security (TLS) port per IANA + Modbus Security spec.
const MODBUS_TLS_PORT: u16 = 802;
/// Standards-defined plain Modbus/TCP port.
const MODBUS_TCP_PORT: u16 = 502;
/// Default port for the out-of-band HTTP control surface (digital-twin).
const CONTROL_PORT: u16 = 8080;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_target(false).init();

    let tls_mode = std::env::var("MODBUS_TLS").ok().as_deref() == Some("1");
    let default_port = if tls_mode {
        MODBUS_TLS_PORT
    } else {
        MODBUS_TCP_PORT
    };
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_port);
    let unit_id: u8 = std::env::var("UNIT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let tick_ms: u64 = std::env::var("TICK_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    // Real meters tolerate one client; the gateway opens a session per
    // measurement and floods on a poll cycle. Allow more concurrent
    // sessions so the smoke doesn't oscillate.
    let max_sessions: usize = std::env::var("MAX_SESSIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64);
    let control_port: u16 = std::env::var("CONTROL_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(CONTROL_PORT);
    let profile = std::env::var("MODBUS_PROFILE").unwrap_or_else(|_| "poi_meter".into());
    let (holding, input, battery) = match profile.as_str() {
        "poi_meter" => (registers::holding_registers(), HashMap::new(), None),
        "bess_rack" => {
            let (battery, holding) = battery::from_env()?;
            (holding, HashMap::new(), Some(battery))
        }
        "dc_external" => (
            dc_external::holding_registers(),
            dc_external::input_registers(),
            None,
        ),
        "pv_inverter" => (pv_inverter::holding_registers(), HashMap::new(), None),
        other => return Err(format!("unknown MODBUS_PROFILE: {other}").into()),
    };
    // Devices whose templates carry commands take writes; meters and the
    // inverter are read-only unless MODBUS_WRITABLE opts them in.
    let poi_meter = profile == "poi_meter";
    let writable = matches!(profile.as_str(), "bess_rack" | "dc_external")
        || std::env::var("MODBUS_WRITABLE").ok().as_deref() == Some("1");

    let mut meter = MeterHandler::new(holding);
    meter.input = input;
    meter.writable = writable;
    let handler = meter.wrap();
    let map = ServerHandlerMap::single(UnitId::new(unit_id), handler.clone());
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);

    // Hoist the server handle to outer scope. rodbus's
    // spawn_tcp_server_task returns a ServerHandle that, when dropped,
    // tears down the listener — so binding inside the else block (which
    // we used to) caused the handle to drop at end-of-block, killing the
    // listener microseconds after the "listening" log. ctrl_c then
    // waited forever on a dead server. Container stayed "Up" but port
    // 502 silently closed → gateway got connection-refused on every poll.
    // Caught via platform-api e2e_defense pipeline 2563801174 stack
    // smoke-defense-348a8f8d (compose-state.log diagnostic).
    let _server_handle = if tls_mode {
        info!(%addr, unit_id, tick_ms, max_sessions, writable, "mock-modbus-server (TLS) listening");
        tls::spawn_tls(addr, map, max_sessions).await?
    } else {
        info!(%addr, unit_id, tick_ms, max_sessions, writable, %profile, "mock-modbus-server (plain) listening");
        spawn_tcp_server_task(
            max_sessions,
            addr,
            map,
            AddressFilter::Any,
            DecodeLevel::default(),
        )
        .await?
    };

    control::spawn_control(handler.clone(), control_port).await?;
    // Reason: the sawtooth and the POI tracker write poi_meter registers;
    // run them for that profile only.
    if poi_meter {
        poi::spawn_from_env(handler.clone(), Duration::from_millis(tick_ms))?;
    }
    if poi_meter || battery.is_some() {
        spawn_simulator(handler, tick_ms, battery);
    }
    tokio::signal::ctrl_c().await?;
    Ok(())
}

/// Run the simulator tick in the background — drifts holding-register values
/// so a polling gateway sees data move. A `bess_rack` profile steps its
/// battery model instead of the poi_meter sawtooth.
fn spawn_simulator(
    handler: Arc<Mutex<Box<MeterHandler>>>,
    tick_ms: u64,
    mut battery: Option<battery::Battery>,
) {
    tokio::spawn(async move {
        let sim = Simulator::new();
        let dt = Duration::from_millis(tick_ms);
        loop {
            {
                let mut guard = handler.lock().unwrap();
                // Reason: split borrow — holding mutably, driven immutably,
                // both fields of the same MeterHandler behind the guard.
                let h = &mut **guard;
                match battery.as_mut() {
                    Some(b) => b.step(&mut h.holding, &h.driven, dt),
                    None => sim.tick(&mut h.holding, &h.driven),
                }
            }
            tokio::time::sleep(Duration::from_millis(tick_ms)).await;
        }
    });
}
