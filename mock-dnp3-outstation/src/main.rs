//! mock-dnp3-outstation — DNP3 TCP / DNP3 over TLS server fixture.
//! Serves a SEL-351 protective relay's DNP3 points (see `sel351`); phase A
//! current (AI 0) is driven by a simulator.
//!
//! Modes selected by env:
//! - `DNP3_TLS=1` → DNP3/TLS (CA-validated mTLS, TLS 1.3).
//!   Requires `DNP3_TLS_CA`, `DNP3_TLS_CERT`, `DNP3_TLS_KEY` paths.
//!   Default port: 19999 (per IEEE 1815 Annex E).
//! - else → plain DNP3/TCP. Default port: 20000.

mod control;
#[cfg(test)]
mod control_roundtrip_test;
mod outstation;
mod sel351;
#[cfg(test)]
mod sel351_test;
mod simulator;

use dnp3::link::LinkErrorMode;
use dnp3::tcp::tls::{MinTlsVersion, TlsServerConfig};
use dnp3::tcp::{AddressFilter, Server};
use outstation::{App, Ctl, Info, NopListener, outstation_config};
use simulator::Simulator;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::info;

/// Standards-defined DNP3/TLS port per IEEE 1815-2012 Annex E.
const DNP3_TLS_PORT: u16 = 19999;
/// De facto DNP3/TCP port (IANA-reserved).
const DNP3_TCP_PORT: u16 = 20000;
/// Default port for the out-of-band HTTP control surface (digital-twin).
const CONTROL_PORT: u16 = 8080;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_target(false).init();

    let tls_mode = std::env::var("DNP3_TLS").ok().as_deref() == Some("1");
    let default_port = if tls_mode {
        DNP3_TLS_PORT
    } else {
        DNP3_TCP_PORT
    };
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default_port);
    let tick_ms: u64 = std::env::var("TICK_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let control_port: u16 = std::env::var("CONTROL_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(CONTROL_PORT);

    let addr = format!("0.0.0.0:{port}").parse()?;
    let mut server = if tls_mode {
        let tls_config = build_tls_server_config()?;
        Server::new_tls_server(LinkErrorMode::Close, addr, tls_config)
    } else {
        Server::new_tcp_server(LinkErrorMode::Close, addr)
    };

    let outstation = server.add_outstation(
        outstation_config(),
        Box::new(App),
        Box::new(Info),
        Box::new(Ctl),
        Box::new(NopListener),
        AddressFilter::Any,
    )?;

    outstation.transaction(sel351::seed);

    let _server_handle = server.bind().await?;
    let mode = if tls_mode { "TLS" } else { "plain" };
    info!(%port, tick_ms, mode, "mock-dnp3-outstation listening");

    let driven: control::DrivenSet = Arc::new(Mutex::new(HashSet::new()));
    control::spawn_control(
        control::ControlState {
            outstation: outstation.clone(),
            driven: driven.clone(),
        },
        control_port,
    )
    .await?;

    // Simulator tick — skips control-driven points.
    let mut sim = Simulator::new();
    let outstation_for_sim = outstation.clone();
    tokio::spawn(async move {
        loop {
            {
                let owned = driven.lock().expect("driven lock poisoned");
                sim.tick(&outstation_for_sim, &owned);
            }
            tokio::time::sleep(Duration::from_millis(tick_ms)).await;
        }
    });

    tokio::signal::ctrl_c().await?;
    Ok(())
}

/// Build the DNP3/TLS server config from env-supplied cert paths.
/// `client_subject_name=None` → accept any CA-validated client cert (no SAN/CN
/// match required — symmetric to mock-modbus-server's posture).
fn build_tls_server_config() -> Result<TlsServerConfig, Box<dyn std::error::Error>> {
    let ca = require_env_path("DNP3_TLS_CA")?;
    let cert = require_env_path("DNP3_TLS_CERT")?;
    let key = require_env_path("DNP3_TLS_KEY")?;
    let tls_config = TlsServerConfig::full_pki(None, &ca, &cert, &key, None, MinTlsVersion::V13)?;
    Ok(tls_config)
}

/// Resolve a required env var into a PathBuf, erroring with the var name.
fn require_env_path(var: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let raw = std::env::var(var).map_err(|_| format!("missing required env: {var}"))?;
    Ok(PathBuf::from(raw))
}
