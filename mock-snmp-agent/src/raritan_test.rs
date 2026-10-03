//! The PDU as Raritan PDU2-MIB reports it (PX4, pduId 1, inletId 1), at
//! the OIDs and units in edp-api's pdu.yaml.

use super::{inlet_power, pole};
use crate::oids::initial_values;
use crate::pdu_load::{PduLoad, readings};

/// measurementsInletPoleSensorValue sensor types.
const RMS_CURRENT: u32 = 1;
const RMS_VOLTAGE: u32 = 4;

#[test]
fn the_inlet_reports_power_and_each_poles_current_and_voltage() {
    // Act
    let m = initial_values();
    // Assert — watts, milliamps (3 decimal digits), volts (0 decimal digits)
    assert_eq!(m[&inlet_power()], 10_800);
    assert_eq!(m[&pole(1, RMS_CURRENT)], 15_000);
    assert_eq!(m[&pole(3, RMS_CURRENT)], 14_800);
    assert_eq!(m[&pole(1, RMS_VOLTAGE)], 240);
    assert_eq!(
        inlet_power(),
        vec![1, 3, 6, 1, 4, 1, 13742, 6, 5, 2, 3, 1, 4, 1, 1, 5]
    );
    assert_eq!(
        pole(2, RMS_CURRENT),
        vec![1, 3, 6, 1, 4, 1, 13742, 6, 5, 2, 4, 1, 4, 1, 1, 2, 1]
    );
}

#[test]
fn a_load_following_pdu_reports_the_same_load_on_both_mibs() {
    // Arrange — 7 nodes at 10.5 kW + 3 kW, over 6 PDUs (3 per feed)
    let load = PduLoad {
        nodes: 7.0,
        share: 6.0,
        base_w: 3_000.0,
    };
    // Act
    let r = readings(&load, 10_500.0, &initial_values());
    // Assert — (7 × 10.5 kW + 3 kW) / 6 = 12.75 kW
    assert_eq!(r[&inlet_power()], 12_750);
    let amps = r[&pole(1, RMS_CURRENT)] as f64 / 1000.0;
    assert!((amps - 12_750.0 / 3.0 / 240.0).abs() < 0.01, "{amps}");
}
