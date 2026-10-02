//! gpu_node's Redfish resources, shaped to edp-api's gpu_node.yaml
//! json_pointers: node power, fan and inlet sensor on `/Chassis/1`, and the
//! eight GPUs on the HGX baseboard (see `gpu_processors`).
//!
//! Readings are flat constants on purpose: a GPU training run holds near-flat
//! at high utilization. Each GPU's power cap is writable, as on a DGX B200:
//! PATCH its EnvironmentMetrics `PowerLimitWatts/SetPoint` (200–1000 W) and
//! it throttles. Tune via env, no rebuild:
//! `GPU_DEMAND_W` (default 1000), `GPU_POWER_LIMIT_W` (1000),
//! `GPU_MAX_CLOCK_MHZ` (1980), `GPU_NODE_OVERHEAD_W` (2500),
//! `GPU_NODE_POWER_LIMIT_W` (26400), `GPU_NODE_INLET_C` (25),
//! `GPU_NODE_FAN_PERCENT` (45).
//!
//! Power figures are edp-module-assemblies CMP-NODE-001 (8× B200 HGX): 1000 W
//! GPU TDP, 10.5 kW typical node draw at full load (so ~2.5 kW is the rest of
//! the node), and a 4× 6600 W PSU nameplate limit. The cap defaults to the
//! TDP, so GPUs boot unthrottled at full load; GPU_POWER_LIMIT_W sets every
//! GPU's boot cap. Clock, temperature and fan duty are
//! illustrative only; the spec gives no figures for them.

use crate::gpu_processors::{Gpu, environment_metrics_json, processor_metrics_json};
use axum::http::StatusCode;
use axum::{Json, Router, routing::get};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

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
                set_point_w: env_f64("GPU_POWER_LIMIT_W", 1_000.0)?,
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

/// The node as served: fixed readings plus each GPU's own, writable cap.
struct Node {
    /// Fixed readings.
    chassis: GpuChassis,
    /// GPU_SXM_1..8, each capped independently by a PATCH.
    gpus: Mutex<[Gpu; GPUS as usize]>,
}

/// Routes for the node's resources and each GPU's. GPU caps are writable
/// (PATCH EnvironmentMetrics), the rest is read-only.
pub fn router(chassis: GpuChassis) -> Router {
    let node = Arc::new(Node {
        gpus: Mutex::new([chassis.gpu; GPUS as usize]),
        chassis,
    });
    let mut router = Router::new()
        .route(
            "/redfish/v1/Chassis/1/Power",
            get(serve(&node, |n, gpus| power_json(&n.chassis, gpus))),
        )
        .route(
            "/redfish/v1/Chassis/1/Thermal",
            get(serve(&node, |n, _| thermal_json(&n.chassis))),
        )
        .route(
            "/redfish/v1/Chassis/1/Sensors/InletTemp",
            get(serve(&node, |n, _| inlet_sensor_json(&n.chassis))),
        );
    for i in 0..usize::from(GPUS) {
        let base = format!(
            "/redfish/v1/Systems/HGX_Baseboard_0/Processors/GPU_SXM_{}",
            i + 1
        );
        let patched = node.clone();
        router = router
            .route(
                &format!("{base}/EnvironmentMetrics"),
                get(serve_gpu(&node, i, environment_metrics_json)).patch(
                    move |Json(body): Json<Value>| async move {
                        let mut gpus = patched.gpus.lock().unwrap();
                        match gpus[i].set_power_limit(&body) {
                            Ok(()) => (StatusCode::OK, Json(environment_metrics_json(&gpus[i]))),
                            Err(reason) => {
                                (StatusCode::BAD_REQUEST, Json(json!({ "error": reason })))
                            }
                        }
                    },
                ),
            )
            .route(
                &format!("{base}/ProcessorMetrics"),
                get(serve_gpu(&node, i, processor_metrics_json)),
            );
    }
    router
}

/// An axum handler rendering `render` over the node's current state.
fn serve(
    node: &Arc<Node>,
    render: fn(&Node, &[Gpu]) -> Value,
) -> impl Fn() -> std::future::Ready<Json<Value>> + Clone + Send + Sync + 'static {
    let node = node.clone();
    move || std::future::ready(Json(render(&node, &*node.gpus.lock().unwrap())))
}

/// An axum handler rendering `render` over GPU `i`'s current state.
fn serve_gpu(
    node: &Arc<Node>,
    i: usize,
    render: fn(&Gpu) -> Value,
) -> impl Fn() -> std::future::Ready<Json<Value>> + Clone + Send + Sync + 'static {
    let node = node.clone();
    move || std::future::ready(Json(render(&node.gpus.lock().unwrap()[i])))
}

/// Redfish Power resource; node draw is the GPUs plus everything else.
pub fn power_json(c: &GpuChassis, gpus: &[Gpu]) -> Value {
    let gpu_w: f64 = gpus
        .iter()
        .filter_map(|g| environment_metrics_json(g)["PowerWatts"]["Reading"].as_f64())
        .sum();
    json!({
        "@odata.id": "/redfish/v1/Chassis/1/Power",
        "@odata.type": "#Power.v1_7_1.Power",
        "Id": "Power",
        "Name": "Power",
        "PowerControl": [{
            "MemberId": "0",
            "Name": "Chassis Power Control",
            "PowerConsumedWatts": gpu_w + c.overhead_w,
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
