//! mock-redfish-service — Redfish HTTP server fixture for gateway testing.
//! Serves a `/redfish/v1/Chassis/SW1/Thermal` resource backed by a ticking
//! simulator.

mod simulator;

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use serde_json::{Value, json};
use simulator::{Simulator, Thermal};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::info;

/// Shared state passed into the axum handlers.
type ThermalState = Arc<Mutex<Thermal>>;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_target(false).init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8443);
    let tick_ms: u64 = std::env::var("TICK_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let state: ThermalState = Arc::new(Mutex::new(Thermal::new()));

    // Simulator tick task.
    let sim_state = state.clone();
    tokio::spawn(async move {
        let sim = Simulator::new();
        loop {
            {
                let mut t = sim_state.lock().await;
                sim.tick(&mut t);
            }
            tokio::time::sleep(Duration::from_millis(tick_ms)).await;
        }
    });

    let app = Router::new()
        .route("/redfish/v1/Chassis/SW1/Thermal", get(thermal_handler))
        .with_state(state);

    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    info!(%port, tick_ms, "mock-redfish-service listening");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Render the current Thermal resource per Redfish DSP0266 §Thermal schema.
async fn thermal_handler(State(state): State<ThermalState>) -> axum::Json<Value> {
    let t = state.lock().await;
    axum::Json(json!({
        "@odata.id": "/redfish/v1/Chassis/SW1/Thermal",
        "@odata.type": "#Thermal.v1_7_0.Thermal",
        "Id": "Thermal",
        "Name": "Thermal",
        "Temperatures": [
            { "Name": "Inlet", "ReadingCelsius": t.inlet_temp },
            { "Name": "ASIC", "ReadingCelsius": t.asic_temp },
        ],
        "Fans": [
            { "Name": "Fan1", "Reading": t.fan_speed, "ReadingUnits": "Percent" },
        ],
    }))
}
