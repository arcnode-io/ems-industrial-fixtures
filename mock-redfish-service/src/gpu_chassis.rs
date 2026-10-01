//! gpu_node's Redfish resources, shaped to edp-api's gpu_node.yaml
//! json_pointers: node power, fan and inlet sensor on `/Chassis/1`, and the
//! eight GPUs on the HGX baseboard (see `gpu_processors`).
//!
//! Readings are flat constants on purpose: a GPU training run holds near-flat
//! at high utilization, which is the demo's point (compute keeps running
//! through a curtailment while the BESS absorbs it). Tune via env, no rebuild:
//! `GPU_DEMAND_W` (default 1000), `GPU_POWER_LIMIT_W` (700),
//! `GPU_MAX_CLOCK_MHZ` (1980), `GPU_NODE_OVERHEAD_W` (2500),
//! `GPU_NODE_POWER_LIMIT_W` (26400), `GPU_NODE_INLET_C` (25),
//! `GPU_NODE_FAN_PERCENT` (45).
//!
//! Power figures are edp-module-assemblies CMP-NODE-001 (8× B200 HGX): 1000 W
//! GPU TDP, 10.5 kW typical node draw at full load (so ~2.5 kW is the rest of
//! the node), and a 4× 6600 W PSU nameplate limit. The 700 W GPU cap is the
//! power-engineer's throttle scenario. Clock, temperature and fan duty are
//! illustrative only; the spec gives no figures for them.

use crate::gpu_processors::{Gpu, environment_metrics_json, processor_metrics_json};
use axum::{Json, Router, routing::get};
use serde_json::{Value, json};
use std::sync::Arc;

/// GPUs on the HGX baseboard.
const GPUS: u8 = 8;

/// Readings the node reports.
#[derive(Debug, Clone, Copy)]
pub struct GpuChassis {
    /// Every GPU's operating point (all eight run the same workload).
    pub gpu: Gpu,
    /// Draw of everything but the GPUs: CPUs, NICs, fans.
    pub overhead_w: f64,
    /// Node BMC power cap.
    pub power_limit_w: f64,
    /// Inlet temperature.
    pub inlet_c: f64,
    /// Fan duty.
    pub fan_percent: f64,
}

impl GpuChassis {
    /// Read readings from env, falling back to the defaults above.
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            gpu: Gpu {
                demand_w: env_f64("GPU_DEMAND_W", 1_000.0)?,
                set_point_w: env_f64("GPU_POWER_LIMIT_W", 700.0)?,
                max_clock_mhz: env_f64("GPU_MAX_CLOCK_MHZ", 1_980.0)?,
            },
            overhead_w: env_f64("GPU_NODE_OVERHEAD_W", 2_500.0)?,
            power_limit_w: env_f64("GPU_NODE_POWER_LIMIT_W", 26_400.0)?,
            inlet_c: env_f64("GPU_NODE_INLET_C", 25.0)?,
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

/// Routes for the node's resources and each GPU's, over fixed readings.
pub fn router(chassis: GpuChassis) -> Router {
    let c = Arc::new(chassis);
    let mut router = Router::new()
        .route("/redfish/v1/Chassis/1/Power", get(serve(&c, power_json)))
        .route(
            "/redfish/v1/Chassis/1/Thermal",
            get(serve(&c, thermal_json)),
        )
        .route(
            "/redfish/v1/Chassis/1/Sensors/InletTemp",
            get(serve(&c, inlet_sensor_json)),
        );
    for n in 1..=GPUS {
        let base = format!("/redfish/v1/Systems/HGX_Baseboard_0/Processors/GPU_SXM_{n}");
        router = router
            .route(
                &format!("{base}/EnvironmentMetrics"),
                get(serve(&c, |c| environment_metrics_json(&c.gpu))),
            )
            .route(
                &format!("{base}/ProcessorMetrics"),
                get(serve(&c, |c| processor_metrics_json(&c.gpu))),
            );
    }
    router
}

/// An axum handler rendering `render` over the shared readings.
fn serve(
    c: &Arc<GpuChassis>,
    render: fn(&GpuChassis) -> Value,
) -> impl Fn() -> std::future::Ready<Json<Value>> + Clone + Send + Sync + 'static {
    let c = c.clone();
    move || std::future::ready(Json(render(&c)))
}

/// Redfish Power resource; node draw is the GPUs plus everything else.
pub fn power_json(c: &GpuChassis) -> Value {
    let per_gpu = environment_metrics_json(&c.gpu)["PowerWatts"]["Reading"]
        .as_f64()
        .unwrap_or_default();
    json!({
        "@odata.id": "/redfish/v1/Chassis/1/Power",
        "@odata.type": "#Power.v1_7_1.Power",
        "Id": "Power",
        "Name": "Power",
        "PowerControl": [{
            "MemberId": "0",
            "Name": "Chassis Power Control",
            "PowerConsumedWatts": f64::from(GPUS) * per_gpu + c.overhead_w,
            "PowerLimit": { "LimitInWatts": c.power_limit_w },
        }],
    })
}

/// Redfish Thermal resource (fans only; temperatures are Sensors).
pub fn thermal_json(c: &GpuChassis) -> Value {
    json!({
        "@odata.id": "/redfish/v1/Chassis/1/Thermal",
        "@odata.type": "#Thermal.v1_7_0.Thermal",
        "Id": "Thermal",
        "Name": "Thermal",
        "Fans": [
            { "MemberId": "0", "Name": "Fan 1", "Reading": c.fan_percent, "ReadingUnits": "Percent" },
        ],
    })
}

/// Redfish Sensor resource for the inlet temperature.
pub fn inlet_sensor_json(c: &GpuChassis) -> Value {
    json!({
        "@odata.id": "/redfish/v1/Chassis/1/Sensors/InletTemp",
        "@odata.type": "#Sensor.v1_7_0.Sensor",
        "Id": "InletTemp",
        "Name": "Inlet Temp",
        "ReadingType": "Temperature",
        "ReadingUnits": "Cel",
        "Reading": c.inlet_c,
    })
}

#[cfg(test)]
#[path = "gpu_chassis_test.rs"]
mod tests;
