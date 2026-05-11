//! mock-modbus-server — Modbus TCP server fixture for gateway testing.
//! Reads canned holding registers per the revenue_meter binding.

mod handler;
mod registers;

use handler::MeterHandler;
use rodbus::server::{spawn_tcp_server_task, AddressFilter, RequestHandler, ServerHandlerMap};
use rodbus::{DecodeLevel, UnitId};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
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

    let handler = MeterHandler::new(registers::holding_registers()).wrap();
    let map = ServerHandlerMap::single(UnitId::new(unit_id), handler);
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);

    info!(%addr, unit_id, "mock-modbus-server listening");

    let _server = spawn_tcp_server_task(
        1,
        addr,
        map,
        AddressFilter::Any,
        DecodeLevel::default(),
    )
    .await?;

    tokio::signal::ctrl_c().await?;
    Ok(())
}
