//! gpu_node's per-GPU resources on the HGX baseboard
//! (`/Systems/HGX_Baseboard_0/Processors/GPU_SXM_{1..8}/{EnvironmentMetrics,
//! ProcessorMetrics}`), shaped to edp-api's gpu_node.yaml json_pointers.
//!
//! Each GPU has a demand and a power cap (`PowerLimitWatts/SetPoint`). Under
//! the cap it draws its demand at full clock; over it, it sits at the cap,
//! its clock drops in proportion and NVIDIA reports `SWPowerCap`, which is
//! the demo's throttle beat.

use serde_json::{Value, json};

/// One GPU's operating point.
#[derive(Debug, Clone, Copy)]
pub struct Gpu {
    /// What the workload would draw uncapped.
    pub demand_w: f64,
    /// The BMC power cap.
    pub set_point_w: f64,
    /// Clock when not throttled.
    pub max_clock_mhz: f64,
}

/// Redfish EnvironmentMetrics for one GPU.
pub fn environment_metrics_json(g: &Gpu) -> Value {
    json!({
        "@odata.type": "#EnvironmentMetrics.v1_3_0.EnvironmentMetrics",
        "Id": "EnvironmentMetrics",
        "PowerWatts": { "Reading": g.power_w() },
        "PowerLimitWatts": { "SetPoint": g.set_point_w, "ControlMode": "Manual" },
    })
}

/// Redfish ProcessorMetrics for one GPU, with NVIDIA's throttle reasons.
pub fn processor_metrics_json(g: &Gpu) -> Value {
    let reason = if g.capped() { "SWPowerCap" } else { "NA" };
    json!({
        "@odata.type": "#ProcessorMetrics.v1_6_0.ProcessorMetrics",
        "Id": "ProcessorMetrics",
        "OperatingSpeedMHz": g.clock_mhz(),
        "Oem": { "Nvidia": { "ThrottleReasons": [reason] } },
    })
}

impl Gpu {
    /// Whether the cap is holding the GPU below its demand.
    fn capped(&self) -> bool {
        self.set_point_w < self.demand_w
    }

    /// Draw: the demand, or the cap when it binds.
    fn power_w(&self) -> f64 {
        self.demand_w.min(self.set_point_w)
    }

    /// Clock, scaled down in proportion to the power the cap takes away.
    fn clock_mhz(&self) -> f64 {
        (self.max_clock_mhz * self.power_w() / self.demand_w).round()
    }
}

#[cfg(test)]
#[path = "gpu_processors_test.rs"]
mod tests;
