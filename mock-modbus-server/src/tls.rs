//! Modbus Security (TLS + Role authz) listener.

use rodbus::DecodeLevel;
use rodbus::server::{
    AddressFilter, CertificateMode, MinTlsVersion, ReadOnlyAuthorizationHandler, RequestHandler,
    ServerHandle, ServerHandlerMap, TlsServerConfig, spawn_tls_server_task_with_authz,
};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Build the Modbus Security (TLS + Role authz) server. CA-based mTLS via
/// rodbus's `TlsServerConfig::new(CertificateMode::AuthorityBased)`; client
/// role extracted from the X.509 Modbus Role extension (OID
/// 1.3.6.1.4.1.50316.802.1) and checked by `ReadOnlyAuthorizationHandler`
/// — accepts all reads, denies all writes. Matches Tier 1 gateway scope.
pub async fn spawn_tls<T: RequestHandler>(
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
