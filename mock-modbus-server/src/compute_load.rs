//! Compute's share of the site load, read from a GPU node's Redfish chassis
//! Power the way a meter sees whatever the nodes draw. On when
//! `SITE_COMPUTE_URL` is set; every node runs the same workload, so one node
//! stands for `SITE_COMPUTE_NODES` of them.

/// How many GPU nodes the site load includes.
pub struct ComputeLoad {
    /// GPU nodes on site.
    pub nodes: f64,
}

/// Site load: `fixed_w` (everything but compute), plus compute's nodes at
/// `node_w` each when it's modeled.
pub fn site_load(fixed_w: f64, compute: Option<(&ComputeLoad, f64)>) -> f64 {
    fixed_w + compute.map_or(0.0, |(c, node_w)| c.nodes * node_w)
}

impl ComputeLoad {
    /// `SITE_COMPUTE_URL` + `SITE_COMPUTE_NODES` (default 1), or `None`
    /// when compute isn't modeled.
    pub fn from_env() -> Result<Option<(Self, String)>, String> {
        let Ok(url) = std::env::var("SITE_COMPUTE_URL") else {
            return Ok(None);
        };
        let nodes = match std::env::var("SITE_COMPUTE_NODES") {
            Ok(raw) => raw
                .parse()
                .map_err(|_| format!("SITE_COMPUTE_NODES is not a number: {raw}"))?,
            Err(_) => 1.0,
        };
        Ok(Some((Self { nodes }, url)))
    }
}

/// A node's `PowerControl/0/PowerConsumedWatts`, or `None` if unreadable.
pub async fn node_power(client: &reqwest::Client, url: &str) -> Option<f64> {
    let body: serde_json::Value = client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    body.pointer("/PowerControl/0/PowerConsumedWatts")?.as_f64()
}

#[cfg(test)]
#[path = "compute_load_test.rs"]
mod tests;
