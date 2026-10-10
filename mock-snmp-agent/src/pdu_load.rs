//! Load-following PDU: input power, and the phase currents that carry it,
//! follow the GPU nodes it feeds, as a real PDU measures what flows through
//! it. On when `PDU_LOAD_URL` is set (a node's Redfish chassis Power; every
//! node on the module runs the same workload, so one node stands for all).

use crate::control::{DrivenSet, SharedValues};
use std::collections::HashMap;
use std::time::Duration;
use tracing::warn;

/// What this PDU feeds and how many PDUs share it.
pub struct PduLoad {
    /// GPU nodes on this PDU's compute module.
    pub nodes: f64,
    /// PDUs splitting that load evenly (2N: A and B sides).
    pub share: f64,
    /// Fixed draw of the module's CDU and switches.
    pub base_w: f64,
}

impl PduLoad {
    /// `PDU_LOAD_NODES` (default 7, a compute container), `PDU_LOAD_SHARE`
    /// (default 4, 2N), `PDU_BASE_LOAD_W` (default 3000: CDU + switch).
    fn from_env() -> Result<Self, String> {
        Ok(Self {
            nodes: env_f64("PDU_LOAD_NODES", 7.0)?,
            share: env_f64("PDU_LOAD_SHARE", 4.0)?,
            base_w: env_f64("PDU_BASE_LOAD_W", 3_000.0)?,
        })
    }
}

/// When `PDU_LOAD_URL` is set, follow the node power it serves every second.
/// The OIDs it drives are marked driven so the drift simulator leaves them.
pub fn spawn_from_env(values: SharedValues, driven: DrivenSet) -> Result<(), String> {
    let Ok(url) = std::env::var("PDU_LOAD_URL") else {
        return Ok(());
    };
    let load = PduLoad::from_env()?;
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        loop {
            match node_power(&client, &url).await {
                Ok(node_w) => {
                    let next = readings(&load, node_w, &*values.lock().await);
                    store(values.clone(), driven.clone(), next).await;
                }
                Err(e) => warn!(%url, error = %e, "PDU load: node power unavailable"),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    Ok(())
}

/// Mark `next`'s OIDs driven and store them.
pub async fn store(values: SharedValues, driven: DrivenSet, next: HashMap<Vec<u32>, i64>) {
    let mut d = driven.lock().await;
    d.extend(next.keys().cloned());
    values.lock().await.extend(next);
}

/// A node's `PowerControl/0/PowerConsumedWatts` from its chassis Power.
async fn node_power(client: &reqwest::Client, url: &str) -> Result<f64, String> {
    let body: serde_json::Value = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    body.pointer("/PowerControl/0/PowerConsumedWatts")
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| format!("no PowerConsumedWatts in {url}"))
}

/// Parse an optional f64 env var, falling back to `default` when unset.
fn env_f64(name: &str, default: f64) -> Result<f64, String> {
    match std::env::var(name) {
        Ok(raw) => raw
            .parse()
            .map_err(|_| format!("{name} is not a number: {raw}")),
        Err(_) => Ok(default),
    }
}

/// The OIDs a node draw of `node_w` sets: input power, and each phase's
/// current at the phase voltage in `values`.
pub fn readings(
    load: &PduLoad,
    node_w: f64,
    values: &HashMap<Vec<u32>, i64>,
) -> HashMap<Vec<u32>, i64> {
    let power_w = (load.nodes * node_w + load.base_w) / load.share;
    let mut out = HashMap::from([(sentry4(3, &[]), power_w.round() as i64)]);
    for phase in 1..=3 {
        // Phase voltage in tenths of a volt; an even split across phases at
        // unity power factor, so Σ V × I is the input power.
        let volts = values.get(&sentry4(5, &[phase])).copied().unwrap_or(2400) as f64 / 10.0;
        let hundredths_a = power_w / 3.0 / volts * 100.0;
        out.insert(sentry4(4, &[phase]), hundredths_a.round() as i64);
    }
    let raritan = crate::raritan::mirror(&out);
    out.extend(raritan);
    out
}

/// Sentry4-MIB OID: …1718.4.1.{table}.3.1.3.1.1{.tail}.
fn sentry4(table: u32, tail: &[u32]) -> Vec<u32> {
    [
        &[1, 3, 6, 1, 4, 1, 1718, 4, 1, table, 3, 1, 3, 1, 1][..],
        tail,
    ]
    .concat()
}

#[cfg(test)]
#[path = "pdu_load_test.rs"]
mod tests;
