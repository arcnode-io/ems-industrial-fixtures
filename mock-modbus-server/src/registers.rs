//! Canned holding-register map for the poi_meter template (Schneider
//! ION9000, Modbus map 004.005.000), one entry per binding in edp-api's
//! poi_meter.yaml, so every measurement reads a value instead of an
//! IllegalDataAddress exception. All FC3, big-endian words (high_low).
//!
//! | measurement    | addr        | type    | value               |
//! |----------------|-------------|---------|---------------------|
//! | active_power   | 3060-3061   | float32 | live, + import W    |
//! | power_factor   | 3150-3151   | float32 | 0.98                |
//! | kwh_delivered  | 3204-3207   | int64   | 1_000_000 Wh        |
//! | kwh_received   | 3208-3211   | int64   | 250_000 Wh          |
//! | thd_voltage_a  | 21330-21331 | float32 | 2.10 %              |
//! | thd_voltage_b  | 21332-21333 | float32 | 2.35 %              |
//! | thd_voltage_c  | 21334-21335 | float32 | 1.95 %              |
//!
//! kwh_delivered's exact value is what the gateway e2e asserts on; the
//! simulator drifts it through the int64's low words. THD phases differ on
//! purpose so a phase-to-address mixup shows up as a wrong number.
//! active_power starts at 0 and is kept live by `poi` (site load minus rack
//! discharge).

use std::collections::HashMap;

/// Build the canned holding-register map.
pub fn holding_registers() -> HashMap<u16, u16> {
    HashMap::from([
        // active_power, float32 0.0
        (3060, 0),
        (3061, 0),
        // power_factor, float32 0.98
        (3150, 0x3F7A),
        (3151, 0xE148),
        // kwh_delivered, int64 1_000_000 = 0x0000_0000_000F_4240
        (3204, 0),
        (3205, 0),
        (3206, 0x000F),
        (3207, 0x4240),
        // kwh_received, int64 250_000 = 0x0000_0000_0003_D090
        (3208, 0),
        (3209, 0),
        (3210, 0x0003),
        (3211, 0xD090),
        // thd_voltage_a/b/c, float32 2.10 / 2.35 / 1.95
        (21330, 0x4006),
        (21331, 0x6666),
        (21332, 0x4016),
        (21333, 0x6666),
        (21334, 0x3FF9),
        (21335, 0x999A),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(m: &HashMap<u16, u16>, addr: u16, n: u16) -> u64 {
        (0..n).fold(0, |acc, i| (acc << 16) | u64::from(m[&(addr + i)]))
    }

    fn f32_at(m: &HashMap<u16, u16>, addr: u16) -> f64 {
        f64::from(f32::from_bits(words(m, addr, 2) as u32))
    }

    #[test]
    fn every_poi_meter_binding_reads_a_plausible_value() {
        // Arrange
        let m = holding_registers();

        // Act + Assert — decoded per edp-api poi_meter.yaml (ION9000 map)
        assert_eq!(
            words(&m, 3204, 4) as i64,
            1_000_000,
            "kwh_delivered int64 Wh"
        );
        assert_eq!(words(&m, 3208, 4) as i64, 250_000, "kwh_received int64 Wh");
        assert!((f32_at(&m, 3150) - 0.98).abs() < 1e-6, "power_factor");
        assert!((f32_at(&m, 21330) - 2.10).abs() < 1e-6, "thd_voltage_a");
        assert!((f32_at(&m, 21332) - 2.35).abs() < 1e-6, "thd_voltage_b");
        assert!((f32_at(&m, 21334) - 1.95).abs() < 1e-6, "thd_voltage_c");
        assert_eq!(f32_at(&m, 3060), 0.0, "active_power");
    }
}
