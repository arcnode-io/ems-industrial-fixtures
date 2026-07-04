//! HTTP control surface — lets an external process (digital-twin) set
//! holding-register values at runtime.
//!
//! One batch endpoint, `PUT /registers`, applying all writes under a single
//! lock so multi-word values (int32 pairs) can never tear mid-Modbus-poll.
//! Out-of-band on purpose: the Modbus surface stays read-only (TLS mode's
//! `ReadOnlyAuthorizationHandler` denies protocol writes by design).
//! Sim-fixture only — never expose this port beyond the deployment network.

use crate::handler::MeterHandler;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::put;
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use tracing::info;

/// Shared handler state — rodbus's `wrap()` shape (note the Box layer).
type SharedHandler = Arc<Mutex<Box<MeterHandler>>>;

/// Batch register write: `{ "registers": { "4000": 15, "4001": 16960 } }`.
#[derive(Deserialize)]
pub struct SetRegisters {
    /// Address -> raw 16-bit value. JSON object keys parse into u16.
    pub registers: HashMap<u16, u16>,
}

/// Build the control router over the shared handler state.
pub fn control_router(handler: SharedHandler) -> Router {
    Router::new()
        .route("/registers", put(put_registers))
        .with_state(handler)
}

/// Apply a batch write atomically; marks addresses control-driven so the
/// simulator stops clobbering them.
async fn put_registers(
    State(handler): State<SharedHandler>,
    Json(body): Json<SetRegisters>,
) -> StatusCode {
    // Reason: std Mutex (rodbus's), never held across an await — lock,
    // mutate, drop within this expression block.
    handler
        .lock()
        .expect("handler mutex poisoned")
        .apply_writes(&body.registers);
    StatusCode::NO_CONTENT
}

/// Spawn the control listener on 0.0.0.0:port. The spawned task owns the
/// listener for the process lifetime (fixture has no shutdown path beyond
/// ctrl_c).
pub async fn spawn_control(
    handler: SharedHandler,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "mock-modbus-server control listening");
    let router = control_router(handler);
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
    use axum::body::Body;
    use axum::http::{Request, header};
    use tower::util::ServiceExt;

    /// Wrap a fresh handler the way rodbus's `wrap()` does.
    fn shared() -> SharedHandler {
        Arc::new(Mutex::new(Box::new(MeterHandler::new(HashMap::new()))))
    }

    #[tokio::test]
    async fn put_registers_applies_batch_and_returns_204() {
        // Arrange
        let handler = shared();
        let router = control_router(handler.clone());
        let request = Request::builder()
            .method("PUT")
            .uri("/registers")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"registers":{"4000":15,"4001":16960}}"#))
            .unwrap();

        // Act
        let response = router.oneshot(request).await.unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let h = handler.lock().unwrap();
        assert_eq!(h.holding.get(&4000), Some(&15));
        assert_eq!(h.holding.get(&4001), Some(&16960));
        assert!(h.driven.contains(&4000));
    }

    #[tokio::test]
    async fn malformed_body_is_rejected() {
        // Arrange
        let router = control_router(shared());
        let request = Request::builder()
            .method("PUT")
            .uri("/registers")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"registers":{"not_an_addr":1}}"#))
            .unwrap();

        // Act
        let response = router.oneshot(request).await.unwrap();

        // Assert — axum Json extractor rejects, no state mutated
        assert!(response.status().is_client_error());
    }
}
