//! mock-redfish-service — Redfish HTTP / HTTPS+mTLS server fixture.
//! Serves `/redfish/v1/Chassis/1/{Power,Thermal,Processors}` (gpu_node) —
//! see `gpu_chassis`.
//!
//! Modes selected by env:
//! - `REDFISH_TLS=1` → HTTPS + CA-validated mTLS (DSP0266 §13.1 + §13.3.5).
//!   Requires `REDFISH_TLS_CA`, `REDFISH_TLS_CERT`, `REDFISH_TLS_KEY` paths.
//!   Default port: 8443.
//! - else → plain HTTP. Default port: 8443 (already HTTP-on-:8443 convention).

mod gpu_chassis;

use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use rustls::RootCertStore;
use rustls::pki_types::CertificateDer;
use rustls::server::WebPkiClientVerifier;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_target(false).init();

    let tls_mode = std::env::var("REDFISH_TLS").ok().as_deref() == Some("1");
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8443);
    let app = Router::new().merge(gpu_chassis::router(gpu_chassis::GpuChassis::from_env()?));

    let addr: SocketAddr = format!("0.0.0.0:{port}").parse()?;
    let mode = if tls_mode { "HTTPS+mTLS" } else { "HTTP" };
    info!(%addr, mode, "mock-redfish-service listening");

    if tls_mode {
        // rustls 0.23 needs a provider; install ring (idempotent).
        let _ = rustls::crypto::ring::default_provider().install_default();
        let tls_config = build_tls_config()?;
        axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;
    }
    Ok(())
}

/// Build a rustls ServerConfig requiring CA-validated client certs per
/// DSP0266 §13.3.5 (mTLS inbound auth).
fn build_tls_config() -> Result<RustlsConfig, Box<dyn std::error::Error>> {
    let ca_path = require_env_path("REDFISH_TLS_CA")?;
    let cert_path = require_env_path("REDFISH_TLS_CERT")?;
    let key_path = require_env_path("REDFISH_TLS_KEY")?;

    let ca_pem = std::fs::read(&ca_path)?;
    let mut roots = RootCertStore::empty();
    for der in rustls_pemfile::certs(&mut ca_pem.as_slice()) {
        roots.add(der?)?;
    }
    let client_verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;

    let cert_pem = std::fs::read(&cert_path)?;
    let server_certs: Vec<CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_pem.as_slice()).collect::<Result<_, _>>()?;
    let key_pem = std::fs::read(&key_path)?;
    let key =
        rustls_pemfile::private_key(&mut key_pem.as_slice())?.ok_or("no private key in key PEM")?;
    let server_config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(server_certs, key)?;
    Ok(RustlsConfig::from_config(Arc::new(server_config)))
}

/// Resolve a required env var into a PathBuf, erroring with the var name.
fn require_env_path(var: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let raw = std::env::var(var).map_err(|_| format!("missing required env: {var}"))?;
    Ok(PathBuf::from(raw))
}
