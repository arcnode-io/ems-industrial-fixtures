//! Unit tests for the gpu_node chassis resources. Each assert uses the exact
//! json_pointer from edp-api's gpu_node.yaml, so a shape drift fails here
//! instead of as a blank HMI row.

use super::{GpuChassis, power_json, processors_json, thermal_json};

fn chassis() -> GpuChassis {
    GpuChassis {
        power_consumed_w: 12_000.0,
        power_limit_w: 14_300.0,
        gpu_power_w: 8_000.0,
        inlet_c: 25.0,
        exhaust_c: 40.0,
        fan_percent: 45.0,
    }
}

#[test]
fn power_resource_serves_consumed_and_limit() {
    let json = power_json(&chassis());
    assert_eq!(
        json.pointer("/PowerControl/0/PowerConsumedWatts"),
        Some(&12_000.0.into())
    );
    assert_eq!(
        json.pointer("/PowerControl/0/PowerLimit/LimitInWatts"),
        Some(&14_300.0.into())
    );
}

#[test]
fn thermal_resource_serves_inlet_exhaust_and_fan() {
    let json = thermal_json(&chassis());
    assert_eq!(
        json.pointer("/Temperatures/0/ReadingCelsius"),
        Some(&25.0.into())
    );
    assert_eq!(
        json.pointer("/Temperatures/1/ReadingCelsius"),
        Some(&40.0.into())
    );
    assert_eq!(json.pointer("/Fans/0/Reading"), Some(&45.0.into()));
}

#[test]
fn processors_resource_serves_nvidia_total_power() {
    let json = processors_json(&chassis());
    assert_eq!(
        json.pointer("/Members/0/Oem/Nvidia/TotalPowerWatts"),
        Some(&8_000.0.into())
    );
}
