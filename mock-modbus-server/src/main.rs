//! mock-modbus-server — Modbus TCP / Modbus Security (TLS+Role) server fixture.
//! Reads holding registers updated on a tick by the `Simulator`.
//!
//! Modes selected by env:
//! - `MODBUS_TLS=1` → Modbus Security (mTLS + CA + Role extension authz).
//!   Requires `MODBUS_TLS_CA`, `MODBUS_TLS_CERT`, `MODBUS_TLS_KEY` paths.
//!   Default port: 802 (per Modbus Security spec).
//! - else → plain Modbus/TCP. Default port: 502.

mod control;
mod handler;
mod registers;
mod simulator;

use handler::MeterHandler;
use rodbus::server::{
    AddressFilter, CertificateMode, MinTlsVersion, ReadOnlyAuthorizationHandler, RequestHandler,
    ServerHandle, ServerHandlerMap, TlsServerConfig, spawn_tcp_server_task,
    spawn_tls_server_task_with_authz,
};
use rodbus::{DecodeLevel, UnitId};
use simulator::Simulator;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
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

    let handler = MeterHandler::new(registers::holding_registers()).wrap();
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
        info!(%addr, unit_id, tick_ms, max_sessions, "mock-modbus-server (TLS) listening");
        spawn_tls(addr, map, max_sessions).await?
    } else {
        info!(%addr, unit_id, tick_ms, max_sessions, "mock-modbus-server (plain) listening");
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
    spawn_simulator(handler, tick_ms);
    tokio::signal::ctrl_c().await?;
    Ok(())
}

/// Build the Modbus Security (TLS + Role authz) server. CA-based mTLS via
/// rodbus's `TlsServerConfig::new(CertificateMode::AuthorityBased)`; client
/// role extracted from the X.509 Modbus Role extension (OID
/// 1.3.6.1.4.1.50316.802.1) and checked by `ReadOnlyAuthorizationHandler`
/// — accepts all reads, denies all writes. Matches Tier 1 gateway scope.
async fn spawn_tls<T: RequestHandler>(
    addr: SocketAddr,
    map: ServerHandlerMap<T>,
    max_sessions: usize,
) -> Result<ServerHandle, Box<dyn std::error::Error>> {
    let ca_bundle = require_env_path("MODBUS_TLS_CA")?;
    let cert = require_env_path("MODBUS_TLS_CERT")?;
    let key = require_env_path("MODBUS_TLS_KEY")?;
    let tls_config = TlsServerConfig::new(
        &ca_bundle,
        &cert,
        &key,
        None,
        MinTlsVersion::V1_3,
        CertificateMode::AuthorityBased,
    )?;
    // Return the ServerHandle so caller can hold it until ctrl_c —
    // dropping it tears down the TLS listener (same bug class as the
    // plain branch). Caller's responsibility to keep the handle alive.
    let server = spawn_tls_server_task_with_authz(
        max_sessions,
        addr,
        map,
        ReadOnlyAuthorizationHandler::create(),
        tls_config,
        AddressFilter::Any,
        DecodeLevel::default(),
    )
    .await?;
    Ok(server)
}

/// Resolve a required env var into a PathBuf, erroring with the var name.
fn require_env_path(var: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let raw = std::env::var(var).map_err(|_| format!("missing required env: {var}"))?;
    Ok(PathBuf::from(raw))
}

/// Run the simulator tick in the background — drifts holding-register values
/// so a polling gateway sees data move.
fn spawn_simulator(handler: Arc<Mutex<Box<MeterHandler>>>, tick_ms: u64) {
    tokio::spawn(async move {
        let sim = Simulator::new();
        loop {
            {
                let mut guard = handler.lock().unwrap();
                // Reason: split borrow — holding mutably, driven immutably,
                // both fields of the same MeterHandler behind the guard.
                let h = &mut **guard;
                sim.tick(&mut h.holding, &h.driven);
            }
            tokio::time::sleep(Duration::from_millis(tick_ms)).await;
        }
    });
}
