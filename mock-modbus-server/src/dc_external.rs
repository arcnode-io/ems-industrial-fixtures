//! Canned registers for the dc_external template (Güntner GFD dry cooler via
//! the GMM EC controller), mirroring edp-api's dc_external.yaml bindings
//! (Modbus GMM V3.0 addresses). Measurements are mostly input registers
//! (FC4); operating_mode and the leaving-fluid setpoint are holding (FC3,
//! written with FC6).
//!
//! | measurement         | fc | addr  | type   | scale | raw | value            |
//! |---------------------|----|-------|--------|-------|-----|------------------|
//! | leaving_fluid_temp  | 4  | 53508 | int16  | 0.1   | 300 | 30.0 °C          |
//! | entering_fluid_temp | 4  | 53511 | int16  | 0.1   | 380 | 38.0 °C          |
//! | ambient_temp        | 4  | 53513 | int16  | 0.1   | 250 | 25.0 °C          |
//! | fan_1_speed         | 4  | 53633 | uint16 | 1     | 65  | 65 %             |
//! | fan_2_speed         | 4  | 53634 | uint16 | 1     | 62  | 62 %             |
//! | fault_word          | 4  | 53616 | uint16 | —     | 0   | no faults        |
//! | operating_mode      | 3  | 53249 | uint16 | enum  | 2   | AUTOMATIC_EXTERNAL_BUS |
//! | set_leaving_fluid_temp (cmd) | 6 | 53257 | uint16 | 0.1 | 300 | 30.0 °C  |
//!
//! Values differ on purpose so an address mixup shows as a wrong number.

use std::collections::HashMap;

/// Holding registers (FC3 reads, FC6 writes).
pub fn holding_registers() -> HashMap<u16, u16> {
    HashMap::from([(53249, 2), (53257, 300)])
}

/// Input registers (FC4 reads).
pub fn input_registers() -> HashMap<u16, u16> {
    HashMap::from([
        (53508, 300),
        (53511, 380),
        (53513, 250),
        (53633, 65),
        (53634, 62),
        (53616, 0),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_dc_external_binding_reads_a_plausible_value() {
        // Arrange
        let input = input_registers();
        let holding = holding_registers();
        let temp = |addr: u16| f64::from(input[&addr] as i16) * 0.1;

        // Act + Assert — decoded per dc_external.yaml's data_type/scale
        assert!((temp(53508) - 30.0).abs() < 1e-9);
        assert!((temp(53511) - 38.0).abs() < 1e-9);
        assert!((temp(53513) - 25.0).abs() < 1e-9);
        assert_eq!(input[&53633], 65);
        assert_eq!(input[&53634], 62);
        assert_eq!(input[&53616], 0);
        assert_eq!(holding[&53249], 2);
        assert!((f64::from(holding[&53257]) * 0.1 - 30.0).abs() < 1e-9);
    }

    #[test]
    fn measurements_are_not_in_the_holding_space() {
        // A gateway reading FC3 by mistake must get an exception, not a value
        assert!(!holding_registers().contains_key(&53508));
    }
}
