//! HTTP control surface — lets an external process (digital-twin) set
//! OID values at runtime.
//!
//! One batch endpoint, `PUT /oids`, applied under a single lock. OIDs are
//! dotted strings on the wire (`"1.3.6.1.4.1.1718.4.1.3.3.1.7"`), values
//! are the raw i64 the agent serves (gateway casts SNMP integers to f64
//! 1:1 — identity scale). Driven OIDs are skipped by the sawtooth
//! simulator. Sim-fixture only — never expose beyond the deployment
//! network.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::put;
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Shared OID value map (same Arc the UDP handler + simulator use).
pub type SharedValues = Arc<Mutex<HashMap<Vec<u32>, i64>>>;
/// OIDs owned by the control surface; the simulator skips these.
pub type DrivenSet = Arc<Mutex<HashSet<Vec<u32>>>>;

/// Control router state.
#[derive(Clone)]
pub struct ControlState {
    /// Live OID → value map.
    pub values: SharedValues,
    /// OIDs the simulator must no longer clobber.
    pub driven: DrivenSet,
}

/// Batch OID write: `{ "values": { "1.3.6.1.4.1.1718.4.1.3.3.1.7": 32 } }`.
#[derive(Deserialize)]
pub struct SetOids {
    /// Dotted OID string -> raw integer value.
    pub values: HashMap<String, i64>,
}

/// Parse a dotted OID string into components; None on any bad segment.
pub fn parse_oid(dotted: &str) -> Option<Vec<u32>> {
    dotted
        .split('.')
        .map(|part| part.parse::<u32>().ok())
        .collect()
}

/// Build the control router over the shared value map + driven set.
pub fn control_router(state: ControlState) -> Router {
    Router::new()
        .route("/oids", put(put_oids))
        .with_state(state)
}

/// Apply a batch write atomically; malformed OIDs reject the whole batch
/// before any mutation (fail fast, no partial application).
async fn put_oids(State(state): State<ControlState>, Json(body): Json<SetOids>) -> StatusCode {
    let mut parsed = Vec::with_capacity(body.values.len());
    for (dotted, value) in &body.values {
        match parse_oid(dotted) {
            Some(oid) => parsed.push((oid, *value)),
            None => return StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
    let mut values = state.values.lock().await;
    let mut driven = state.driven.lock().await;
    for (oid, value) in parsed {
        values.insert(oid.clone(), value);
        driven.insert(oid);
    }
    StatusCode::NO_CONTENT
}

/// Spawn the control listener on 0.0.0.0:port for the process lifetime.
pub async fn spawn_control(
    state: ControlState,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "mock-snmp-agent control listening");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oids::OID_INPUT_CURRENT;
    use axum::body::Body;
    use axum::http::{Request, header};
    use tower::util::ServiceExt;

    /// Fresh empty control state.
    fn state() -> ControlState {
        ControlState {
            values: Arc::new(Mutex::new(HashMap::new())),
            driven: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    #[tokio::test]
    async fn put_oids_applies_batch_and_marks_driven() {
        // Arrange
        let s = state();
        let request = Request::builder()
            .method("PUT")
            .uri("/oids")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"values":{"1.3.6.1.4.1.1718.4.1.3.3.1.7":32}}"#,
            ))
            .unwrap();

        // Act
        let response = control_router(s.clone()).oneshot(request).await.unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let values = s.values.lock().await;
        assert_eq!(values.get(&OID_INPUT_CURRENT.to_vec()), Some(&32));
        assert!(s.driven.lock().await.contains(&OID_INPUT_CURRENT.to_vec()));
    }

    #[tokio::test]
    async fn malformed_oid_rejects_whole_batch() {
        // Arrange — one good, one bad OID in the same batch
        let s = state();
        let request = Request::builder()
            .method("PUT")
            .uri("/oids")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"values":{"1.3.6.1.4.1.1718.4.1.3.3.1.7":32,"not.an.oid":1}}"#,
            ))
            .unwrap();

        // Act
        let response = control_router(s.clone()).oneshot(request).await.unwrap();

        // Assert — rejected, nothing applied
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(s.values.lock().await.is_empty());
        assert!(s.driven.lock().await.is_empty());
    }
}
