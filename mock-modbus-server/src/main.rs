//! mock-modbus-server — Modbus TCP server fixture for gateway testing.
//! Reads holding registers updated on a tick by the `Simulator`.

mod handler;
mod registers;
mod simulator;

use handler::MeterHandler;
use rodbus::server::{spawn_tcp_server_task, AddressFilter, RequestHandler, ServerHandlerMap};
use rodbus::{DecodeLevel, UnitId};
use simulator::Simulator;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tracing::info;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_target(false).init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(502);
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

    let handler = MeterHandler::new(registers::holding_registers()).wrap();
    let map = ServerHandlerMap::single(UnitId::new(unit_id), handler.clone());
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);

    info!(%addr, unit_id, tick_ms, max_sessions, "mock-modbus-server listening");

    let _server = spawn_tcp_server_task(
        max_sessions,
        addr,
        map,
        AddressFilter::Any,
        DecodeLevel::default(),
    )
    .await?;

    // Spawn the simulator tick — runs forever, mutates `handler.holding`.
    let sim_handler = handler.clone();
    tokio::spawn(async move {
        let sim = Simulator::new();
        loop {
            {
                let mut h = sim_handler.lock().unwrap();
                sim.tick(&mut h.holding);
            }
            tokio::time::sleep(Duration::from_millis(tick_ms)).await;
        }
    });

    tokio::signal::ctrl_c().await?;
    Ok(())
}
