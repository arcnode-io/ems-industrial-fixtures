//! Canned OID map for the `pdu` template (Server Technology PRO3X), from
//! Sentry4-MIB (enterprise 1718), mirroring edp-api's pdu.yaml. Values are
//! raw MIB integers; the template's scale turns them into units.
//!
//! | measurement      | OID (…1718.4.1.)       | raw  | scale | value    |
//! |------------------|------------------------|------|-------|----------|
//! | input_current_l1 | 4.3.1.3.1.1.1          | 1500 | 0.01  | 15.00 A  |
//! | input_current_l2 | 4.3.1.3.1.1.2          | 1520 | 0.01  | 15.20 A  |
//! | input_current_l3 | 4.3.1.3.1.1.3          | 1480 | 0.01  | 14.80 A  |
//! | input_voltage_l1 | 5.3.1.3.1.1.1          | 2400 | 0.1   | 240.0 V  |
//! | input_voltage_l2 | 5.3.1.3.1.1.2          | 2395 | 0.1   | 239.5 V  |
//! | input_voltage_l3 | 5.3.1.3.1.1.3          | 2405 | 0.1   | 240.5 V  |
//!
//! st4LineCurrent in hundredths of an amp, st4PhaseVoltage in tenths of a
//! volt (~240 V phase for 415 V line-to-line). Phases differ on purpose so a
//! phase mixup shows as a wrong number. L1 current drifts (see simulator).

use std::collections::HashMap;

/// st4LineCurrent, L1 (1.3.6.1.4.1.1718.4.1.4.3.1.3.1.1.1).
pub const OID_INPUT_CURRENT: &[u32] = &[1, 3, 6, 1, 4, 1, 1718, 4, 1, 4, 3, 1, 3, 1, 1, 1];

/// Build the initial OID → integer-value map.
pub fn initial_values() -> HashMap<Vec<u32>, i64> {
    let sentry4 =
        |table: u32, phase: u32| vec![1, 3, 6, 1, 4, 1, 1718, 4, 1, table, 3, 1, 3, 1, 1, phase];
    HashMap::from([
        (sentry4(4, 1), 1500),
        (sentry4(4, 2), 1520),
        (sentry4(4, 3), 1480),
        (sentry4(5, 1), 2400),
        (sentry4(5, 2), 2395),
        (sentry4(5, 3), 2405),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sentry4-MIB OID under enterprise 1718: …1718.4.1.{table}.3.1.3.1.1.{phase}.
    fn oid(table: u32, phase: u32) -> Vec<u32> {
        vec![1, 3, 6, 1, 4, 1, 1718, 4, 1, table, 3, 1, 3, 1, 1, phase]
    }

    #[test]
    fn every_pdu_binding_reads_a_plausible_value() {
        // Arrange
        let m = initial_values();
        // Act + Assert — scaled per edp-api pdu.yaml
        let amps = |phase| m[&oid(4, phase)] as f64 * 0.01;
        let volts = |phase| m[&oid(5, phase)] as f64 * 0.1;
        assert!((amps(1) - 15.00).abs() < 1e-9 && (amps(2) - 15.20).abs() < 1e-9);
        assert!((amps(3) - 14.80).abs() < 1e-9);
        assert!((volts(1) - 240.0).abs() < 1e-9 && (volts(2) - 239.5).abs() < 1e-9);
        assert!((volts(3) - 240.5).abs() < 1e-9);
        assert_eq!(OID_INPUT_CURRENT.to_vec(), oid(4, 1));
    }
}
