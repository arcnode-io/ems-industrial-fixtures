//! Per-GPU resources, asserted at the exact json_pointers in edp-api's
//! gpu_node.yaml, so a shape drift fails here instead of as a blank HMI row.

use super::{Gpu, environment_metrics_json, processor_metrics_json};

const CAPPED: Gpu = Gpu {
    demand_w: 1000.0,
    set_point_w: 700.0,
    max_clock_mhz: 1980.0,
};
const UNCAPPED: Gpu = Gpu {
    demand_w: 1000.0,
    set_point_w: 1000.0,
    max_clock_mhz: 1980.0,
};

#[test]
fn a_capped_gpu_sits_at_its_set_point_and_reports_sw_power_cap() {
    // Act
    let env = environment_metrics_json(&CAPPED);
    let proc = processor_metrics_json(&CAPPED);
    // Assert — power at the cap, clock down, throttle reason the cap
    assert_eq!(env.pointer("/PowerWatts/Reading"), Some(&700.0.into()));
    assert_eq!(
        env.pointer("/PowerLimitWatts/SetPoint"),
        Some(&700.0.into())
    );
    assert_eq!(proc.pointer("/OperatingSpeedMHz"), Some(&1386.0.into()));
    assert_eq!(
        proc.pointer("/Oem/Nvidia/ThrottleReasons/0"),
        Some(&"SWPowerCap".into())
    );
}

#[test]
fn an_uncapped_gpu_draws_its_demand_at_full_clock() {
    let env = environment_metrics_json(&UNCAPPED);
    let proc = processor_metrics_json(&UNCAPPED);
    assert_eq!(env.pointer("/PowerWatts/Reading"), Some(&1000.0.into()));
    assert_eq!(proc.pointer("/OperatingSpeedMHz"), Some(&1980.0.into()));
    assert_eq!(
        proc.pointer("/Oem/Nvidia/ThrottleReasons/0"),
        Some(&"NA".into())
    );
}
