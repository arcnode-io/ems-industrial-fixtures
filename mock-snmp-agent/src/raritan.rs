//! The PDU as Raritan PDU2-MIB (enterprise 13742) reports it: PX4 and the
//! Xerus family (PRO3X/PRO4X), pduId 1, inletId 1. Mirrors the Sentry4
//! readings, which stay the one source of the PDU's physics.

use std::collections::HashMap;

/// Inlet sensor type: activePower.
const ACTIVE_POWER: u32 = 5;
/// Pole sensor type: rmsCurrent.
const RMS_CURRENT: u32 = 1;
/// Pole sensor type: rmsVoltage.
const RMS_VOLTAGE: u32 = 4;

/// measurementsInletSensorValue, activePower (W).
pub fn inlet_power() -> Vec<u32> {
    vec![
        1,
        3,
        6,
        1,
        4,
        1,
        13742,
        6,
        5,
        2,
        3,
        1,
        4,
        1,
        1,
        ACTIVE_POWER,
    ]
}

/// measurementsInletPoleSensorValue for `pole` (L1..L3) and `sensor`
/// (1 rmsCurrent mA, 4 rmsVoltage V).
pub fn pole(pole: u32, sensor: u32) -> Vec<u32> {
    vec![
        1, 3, 6, 1, 4, 1, 13742, 6, 5, 2, 4, 1, 4, 1, 1, pole, sensor,
    ]
}

/// The PDU2-MIB readings for Sentry4 ones present in `sentry4`.
pub fn mirror(sentry4: &HashMap<Vec<u32>, i64>) -> HashMap<Vec<u32>, i64> {
    let at = |table: u32, tail: &[u32]| {
        let oid = [
            &[1, 3, 6, 1, 4, 1, 1718, 4, 1, table, 3, 1, 3, 1, 1][..],
            tail,
        ]
        .concat();
        sentry4.get(&oid).copied()
    };
    let mut out = HashMap::new();
    if let Some(watts) = at(3, &[]) {
        out.insert(inlet_power(), watts);
    }
    for p in 1..=3 {
        // Sentry4 hundredths of an amp → milliamps; tenths of a volt → volts.
        if let Some(centiamps) = at(4, &[p]) {
            out.insert(pole(p, RMS_CURRENT), centiamps * 10);
        }
        if let Some(decivolts) = at(5, &[p]) {
            out.insert(
                pole(p, RMS_VOLTAGE),
                (decivolts as f64 / 10.0).round() as i64,
            );
        }
    }
    out
}

#[cfg(test)]
#[path = "raritan_test.rs"]
mod tests;
