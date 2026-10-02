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

#[test]
fn a_patched_set_point_caps_the_gpu() {
    // Arrange
    let mut gpu = UNCAPPED;
    // Act — the gateway's PATCH body
    let applied =
        gpu.set_power_limit(&serde_json::json!({ "PowerLimitWatts": { "SetPoint": 810.0 } }));
    // Assert
    assert_eq!(applied, Ok(()));
    assert_eq!(
        environment_metrics_json(&gpu).pointer("/PowerWatts/Reading"),
        Some(&810.0.into())
    );
}

#[test]
fn a_set_point_outside_the_allowable_range_is_refused() {
    // Arrange — DGX B200: 200–1000 W, advertised on the resource
    let mut gpu = UNCAPPED;
    let env = environment_metrics_json(&gpu);
    assert_eq!(
        env.pointer("/PowerLimitWatts/AllowableMin"),
        Some(&200.0.into())
    );
    assert_eq!(
        env.pointer("/PowerLimitWatts/AllowableMax"),
        Some(&1000.0.into())
    );
    // Act + Assert — refused, cap unchanged
    for bad in [150.0, 1200.0] {
        let body = serde_json::json!({ "PowerLimitWatts": { "SetPoint": bad } });
        assert!(gpu.set_power_limit(&body).is_err(), "{bad} W accepted");
    }
    assert!(
        gpu.set_power_limit(&serde_json::json!({ "Other": 1 }))
            .is_err()
    );
    assert_eq!(gpu.set_point_w, 1000.0);
}
