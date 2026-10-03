//! Canned OID map for the `pdu` template (Sentry4-MIB, enterprise 1718,
//! also served as Raritan PDU2-MIB, see `raritan`) and the `network_switch` template (NVIDIA SN5600 on
//! Cumulus: IF-MIB + ENTITY-SENSOR-MIB), mirroring edp-api's pdu.yaml and
//! network_switch.yaml. Values are raw MIB integers; the template's scale
//! turns them into units. The OID trees don't overlap, so one agent serves
//! both.
//!
//! | measurement      | OID (…1718.4.1.)       | raw  | scale | value    |
//! |------------------|------------------------|------|-------|----------|
//! | input_current_l1 | 4.3.1.3.1.1.1          | 1500 | 0.01  | 15.00 A  |
//! | input_current_l2 | 4.3.1.3.1.1.2          | 1520 | 0.01  | 15.20 A  |
//! | input_current_l3 | 4.3.1.3.1.1.3          | 1480 | 0.01  | 14.80 A  |
//! | input_voltage_l1 | 5.3.1.3.1.1.1          | 2400 | 0.1   | 240.0 V  |
//! | input_voltage_l2 | 5.3.1.3.1.1.2          | 2395 | 0.1   | 239.5 V  |
//! | input_voltage_l3 | 5.3.1.3.1.1.3          | 2405 | 0.1   | 240.5 V  |
//! | input_power      | 3.3.1.3.1.1            | 10800| 1     | 10.8 kW  |
//!
//! st4LineCurrent in hundredths of an amp, st4PhaseVoltage in tenths of a
//! volt (~240 V phase for 415 V line-to-line), st4InputCordActivePower in
//! watts: the phases' V × I at unity power factor. Phases differ on purpose so a
//! phase mixup shows as a wrong number. L1 current drifts (see simulator).
//!
//! | network_switch    | OID (1.3.6.1.2.1.)  | value     |
//! |-------------------|---------------------|-----------|
//! | port_link_status  | 2.2.1.8.1           | 1 (up)    |
//! | inlet_temp        | 99.1.1.1.4.1        | 27 °C     |
//! | asic_temp         | 99.1.1.1.4.2        | 58 °C     |
//!
//! Sensor indices 1/2 and ifIndex 1 are the template's unverified defaults.

use std::collections::HashMap;

/// st4LineCurrent, L1 (1.3.6.1.4.1.1718.4.1.4.3.1.3.1.1.1).
pub const OID_INPUT_CURRENT: &[u32] = &[1, 3, 6, 1, 4, 1, 1718, 4, 1, 4, 3, 1, 3, 1, 1, 1];

/// Build the initial OID → integer-value map.
pub fn initial_values() -> HashMap<Vec<u32>, i64> {
    let mut values = sentry4_and_switch();
    values.extend(crate::raritan::mirror(&values));
    values
}

/// The Sentry4 PDU readings and the switch's, before the Raritan mirror.
fn sentry4_and_switch() -> HashMap<Vec<u32>, i64> {
    let sentry4 =
        |table: u32, phase: u32| vec![1, 3, 6, 1, 4, 1, 1718, 4, 1, table, 3, 1, 3, 1, 1, phase];
    HashMap::from([
        (sentry4(4, 1), 1500),
        (sentry4(4, 2), 1520),
        (sentry4(4, 3), 1480),
        (sentry4(5, 1), 2400),
        (sentry4(5, 2), 2395),
        (sentry4(5, 3), 2405),
        // st4InputCordActivePower: the three phases' V × I, unity PF
        (vec![1, 3, 6, 1, 4, 1, 1718, 4, 1, 3, 3, 1, 3, 1, 1], 10_800),
        // network_switch: ifOperStatus.1 = up(1), entPhySensorValue.1/.2
        (vec![1, 3, 6, 1, 2, 1, 2, 2, 1, 8, 1], 1),
        (vec![1, 3, 6, 1, 2, 1, 99, 1, 1, 1, 4, 1], 27),
        (vec![1, 3, 6, 1, 2, 1, 99, 1, 1, 1, 4, 2], 58),
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

    #[test]
    fn input_power_is_the_cords_three_phases() {
        // Arrange — st4InputCordActivePower, integer watts (scale 1)
        let m = initial_values();
        let cord = vec![1, 3, 6, 1, 4, 1, 1718, 4, 1, 3, 3, 1, 3, 1, 1];
        // Act — what the phases carry, at unity power factor
        let va: f64 = (1..=3)
            .map(|p| m[&oid(4, p)] as f64 * 0.01 * m[&oid(5, p)] as f64 * 0.1)
            .sum();
        // Assert
        assert_eq!(m[&cord], va.round() as i64);
    }

    #[test]
    fn every_network_switch_binding_reads_a_plausible_value() {
        // Arrange — IF-MIB and ENTITY-SENSOR-MIB, per edp-api network_switch.yaml
        let m = initial_values();
        let mib2 = |tail: &[u32]| [&[1, 3, 6, 1, 2, 1][..], tail].concat();
        // Act + Assert
        assert_eq!(m[&mib2(&[2, 2, 1, 8, 1])], 1, "port_link_status UP");
        assert_eq!(m[&mib2(&[99, 1, 1, 1, 4, 1])], 27, "inlet_temp °C");
        assert_eq!(m[&mib2(&[99, 1, 1, 1, 4, 2])], 58, "asic_temp °C");
    }
}
