//! HTTP control surface — lets an external process (digital-twin) set
//! analog input points at runtime.
//!
//! One batch endpoint, `PUT /points`, applied in a single outstation
//! transaction. DNP3 carries engineering values (f64) directly — no
//! raw-word encoding like Modbus. Unknown point indices are seeded on
//! demand so a DTM template (e.g. operating_envelope: points 0, 1, 100)
//! can be driven without pre-declaring its map here. Sim-fixture only —
//! never expose this port beyond the deployment network.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::put;
use axum::{Json, Router};
use dnp3::app::Timestamp;
use dnp3::app::measurement::{AnalogInput, Flags, Time};
use dnp3::outstation::OutstationHandle;
use dnp3::outstation::database::{
    Add, AnalogInputConfig, EventAnalogInputVariation, EventClass, StaticAnalogInputVariation,
    Update, UpdateOptions,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tracing::info;

/// Point indices owned by the control surface; the simulator skips these.
pub type DrivenSet = Arc<Mutex<HashSet<u16>>>;

/// Shared state for the control router.
#[derive(Clone)]
pub struct ControlState {
    /// Handle into the outstation database.
    pub outstation: OutstationHandle,
    /// Indices the simulator must no longer clobber.
    pub driven: DrivenSet,
}

/// Batch point write: `{ "analog_inputs": { "0": 5000000.0, "100": 0.0 } }`.
#[derive(Deserialize)]
pub struct SetPoints {
    /// Point index -> engineering value.
    pub analog_inputs: HashMap<u16, f64>,
}

/// Build the control router over the outstation handle + driven set.
pub fn control_router(state: ControlState) -> Router {
    Router::new()
        .route("/points", put(put_points))
        .with_state(state)
}

/// Apply a batch write in one transaction; seed unknown indices on demand
/// and mark every index control-driven.
async fn put_points(State(state): State<ControlState>, Json(body): Json<SetPoints>) -> StatusCode {
    state.outstation.transaction(|db| {
        for (&index, &value) in &body.analog_inputs {
            let sample = AnalogInput::new(value, Flags::ONLINE, current_time());
            if !db.update(index, &sample, UpdateOptions::detect_event()) {
                db.add(
                    index,
                    Some(EventClass::Class1),
                    AnalogInputConfig {
                        s_var: StaticAnalogInputVariation::Group30Var1,
                        e_var: EventAnalogInputVariation::Group32Var1,
                        deadband: 0.0,
                    },
                );
                db.update(index, &sample, UpdateOptions::detect_event());
            }
        }
    });
    let mut driven = state.driven.lock().expect("driven lock poisoned");
    driven.extend(body.analog_inputs.keys());
    StatusCode::NO_CONTENT
}

/// Spawn the control listener on 0.0.0.0:port for the process lifetime.
pub async fn spawn_control(
    state: ControlState,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "mock-dnp3-outstation control listening");
    let router = control_router(state);
    tokio::spawn(async move {
        // Reason: serve() only errors on accept-loop failure; the fixture
        // has no recovery story beyond crashing loudly in the logs.
        if let Err(err) = axum::serve(listener, router).await {
            tracing::error!(%err, "control surface died");
        }
    });
    Ok(())
}

/// Wall-clock time as a DNP3 Synchronized timestamp.
fn current_time() -> Time {
    let epoch = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch");
    Time::Synchronized(Timestamp::new(epoch.as_millis() as u64))
}
