//! gpu_node chassis resources (`/Chassis/1/{Power,Thermal,Processors}`),
//! shaped to edp-api's gpu_node.yaml json_pointers.
//!
//! Readings are flat constants on purpose: a GPU training run holds near-flat
//! at high utilization, which is the demo's point (compute keeps running
//! through a curtailment while the BESS absorbs it). Tune via env, no rebuild:
//! `GPU_NODE_POWER_W` (default 10500), `GPU_NODE_POWER_LIMIT_W` (26400),
//! `GPU_TOTAL_POWER_W` (8000), `GPU_NODE_INLET_C` (25), `GPU_NODE_EXHAUST_C`
//! (40), `GPU_NODE_FAN_PERCENT` (45).
//!
//! Power defaults are edp-module-assemblies CMP-NODE-001 (8× B200 HGX):
//! 10.5 kW typical sustained at full GPU load; 8× 1000 W GPU TDP; the limit
//! is the 4× 6600 W PSU nameplate, i.e. an uncapped BMC. Temperatures and fan
//! duty are illustrative only; the spec gives no figures for them.

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};
use std::sync::Arc;

/// Readings the chassis reports.
pub struct GpuChassis {
    /// Whole-node draw.
    pub power_consumed_w: f64,
    /// BMC power cap.
    pub power_limit_w: f64,
    /// Sum across the node's GPUs.
    pub gpu_power_w: f64,
    /// Inlet temperature.
    pub inlet_c: f64,
    /// Exhaust temperature.
    pub exhaust_c: f64,
    /// Fan duty.
    pub fan_percent: f64,
}

impl GpuChassis {
    /// Read readings from env, falling back to the defaults above.
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            power_consumed_w: env_f64("GPU_NODE_POWER_W", 10_500.0)?,
            power_limit_w: env_f64("GPU_NODE_POWER_LIMIT_W", 26_400.0)?,
            gpu_power_w: env_f64("GPU_TOTAL_POWER_W", 8_000.0)?,
            inlet_c: env_f64("GPU_NODE_INLET_C", 25.0)?,
            exhaust_c: env_f64("GPU_NODE_EXHAUST_C", 40.0)?,
            fan_percent: env_f64("GPU_NODE_FAN_PERCENT", 45.0)?,
        })
    }
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

/// Routes for the three chassis resources, over fixed readings.
pub fn router(chassis: GpuChassis) -> Router {
    let c = Arc::new(chassis);
    let (p, t, g) = (c.clone(), c.clone(), c);
    Router::new()
        .route(
            "/redfish/v1/Chassis/1/Power",
            get(move || async move { Json(power_json(&p)) }),
        )
        .route(
            "/redfish/v1/Chassis/1/Thermal",
            get(move || async move { Json(thermal_json(&t)) }),
        )
        .route(
            "/redfish/v1/Chassis/1/Processors",
            get(move || async move { Json(processors_json(&g)) }),
        )
}

/// Redfish Power resource.
pub fn power_json(c: &GpuChassis) -> Value {
    json!({
        "@odata.id": "/redfish/v1/Chassis/1/Power",
        "@odata.type": "#Power.v1_7_1.Power",
        "Id": "Power",
        "Name": "Power",
        "PowerControl": [{
            "MemberId": "0",
            "Name": "Chassis Power Control",
            "PowerConsumedWatts": c.power_consumed_w,
            "PowerLimit": { "LimitInWatts": c.power_limit_w },
        }],
    })
}

/// Redfish Thermal resource.
pub fn thermal_json(c: &GpuChassis) -> Value {
    json!({
        "@odata.id": "/redfish/v1/Chassis/1/Thermal",
        "@odata.type": "#Thermal.v1_7_0.Thermal",
        "Id": "Thermal",
        "Name": "Thermal",
        "Temperatures": [
            { "MemberId": "0", "Name": "Inlet Temp", "ReadingCelsius": c.inlet_c },
            { "MemberId": "1", "Name": "Exhaust Temp", "ReadingCelsius": c.exhaust_c },
        ],
        "Fans": [
            { "MemberId": "0", "Name": "Fan 1", "Reading": c.fan_percent, "ReadingUnits": "Percent" },
        ],
    })
}

/// Redfish Processors collection, with NVIDIA's OEM total GPU power.
pub fn processors_json(c: &GpuChassis) -> Value {
    json!({
        "@odata.id": "/redfish/v1/Chassis/1/Processors",
        "@odata.type": "#ProcessorCollection.ProcessorCollection",
        "Name": "Processors Collection",
        "Members@odata.count": 1,
        "Members": [{
            "@odata.id": "/redfish/v1/Chassis/1/Processors/GPU_SXM_1",
            "Oem": { "Nvidia": { "TotalPowerWatts": c.gpu_power_w } },
        }],
    })
}

#[cfg(test)]
#[path = "gpu_chassis_test.rs"]
mod tests;
