//! Unit tests for the gpu_node chassis resources. Each assert uses the exact
//! json_pointer from edp-api's gpu_node.yaml, so a shape drift fails here
//! instead of as a blank HMI row.

use super::{GpuChassis, inlet_sensor_json, power_json, thermal_json};
use crate::gpu_processors::Gpu;

fn chassis() -> GpuChassis {
    GpuChassis {
        gpu: Gpu {
            demand_w: 1000.0,
            set_point_w: 700.0,
            max_clock_mhz: 1980.0,
        },
        overhead_w: 2_500.0,
        power_limit_w: 26_400.0,
        inlet_c: 25.0,
        fan_percent: 45.0,
    }
}

#[test]
fn node_power_is_its_gpus_plus_the_rest_of_the_node() {
    // Arrange — 8 GPUs capped at 700 W, 2.5 kW for CPUs, NICs, fans
    // Act
    let json = power_json(&chassis());
    // Assert — so the node total and the per-GPU sum agree
    assert_eq!(
        json.pointer("/PowerControl/0/PowerConsumedWatts"),
        Some(&8_100.0.into())
    );
    assert_eq!(
        json.pointer("/PowerControl/0/PowerLimit/LimitInWatts"),
        Some(&26_400.0.into())
    );
}

#[test]
fn thermal_resource_serves_the_fan() {
    let json = thermal_json(&chassis());
    assert_eq!(json.pointer("/Fans/0/Reading"), Some(&45.0.into()));
}

#[test]
fn inlet_temperature_is_its_own_sensor() {
    let json = inlet_sensor_json(&chassis());
    assert_eq!(json.pointer("/Reading"), Some(&25.0.into()));
}
