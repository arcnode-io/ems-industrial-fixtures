//! Canned holding-register map for the poi_meter template — one entry per
//! binding in edp-api's poi_meter.yaml, so every measurement reads a value
//! instead of an IllegalDataAddress exception.
//!
//! | measurement    | addr      | type          | scale | raw      | value      |
//! |----------------|-----------|---------------|-------|----------|------------|
//! | kwh_delivered  | 4000-4001 | int32 hi_lo   | 1.0   | 0x000F4240 | 1_000_000 Wh |
//! | kwh_received   | 4002-4003 | int32 hi_lo   | 1.0   | 0x0003D090 | 250_000 Wh |
//! | power_factor   | 4010      | int16         | 0.001 | 980      | 0.98       |
//! | thd_voltage_a  | 4020      | int16         | 0.01  | 210      | 2.10 %     |
//! | thd_voltage_b  | 4021      | int16         | 0.01  | 235      | 2.35 %     |
//! | thd_voltage_c  | 4022      | int16         | 0.01  | 195      | 1.95 %     |
//!
//! kwh_delivered's exact value is what the gateway e2e asserts on. THD phases
//! differ on purpose so a phase-to-address mixup shows up as a wrong number.

use std::collections::HashMap;

/// Build the canned holding-register map.
pub fn holding_registers() -> HashMap<u16, u16> {
    HashMap::from([
        (4000, 0x000F),
        (4001, 0x4240),
        (4002, 0x0003),
        (4003, 0xD090),
        (4010, 980),
        (4020, 210),
        (4021, 235),
        (4022, 195),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int32_high_low(m: &HashMap<u16, u16>, addr: u16) -> i32 {
        ((u32::from(m[&addr]) << 16) | u32::from(m[&(addr + 1)])) as i32
    }

    fn int16(m: &HashMap<u16, u16>, addr: u16) -> i16 {
        m[&addr] as i16
    }

    #[test]
    fn every_poi_meter_binding_reads_a_plausible_value() {
        // Arrange
        let m = holding_registers();

        // Act — decode each binding per edp-api poi_meter.yaml's data_type/scale
        let kwh_delivered = f64::from(int32_high_low(&m, 4000));
        let kwh_received = f64::from(int32_high_low(&m, 4002));
        let power_factor = f64::from(int16(&m, 4010)) * 0.001;
        let thd_a = f64::from(int16(&m, 4020)) * 0.01;
        let thd_b = f64::from(int16(&m, 4021)) * 0.01;
        let thd_c = f64::from(int16(&m, 4022)) * 0.01;

        // Assert
        assert_eq!(kwh_delivered, 1_000_000.0);
        assert_eq!(kwh_received, 250_000.0);
        assert!((power_factor - 0.98).abs() < 1e-9);
        assert!((thd_a - 2.10).abs() < 1e-9);
        assert!((thd_b - 2.35).abs() < 1e-9);
        assert!((thd_c - 1.95).abs() < 1e-9);
    }
}
