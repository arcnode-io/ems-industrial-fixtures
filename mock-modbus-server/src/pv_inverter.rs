//! Canned registers for the pv_inverter template: a SunSpec device with the
//! common model (1) and the three-phase inverter model (103), mirroring
//! edp-api's pv_inverter.yaml. The gateway reads W with its scale factor.
//!
//! | what         | addr        | value            |
//! |--------------|-------------|------------------|
//! | SunS marker  | 40000-40001 | 0x5375 0x6E53    |
//! | model 1 ID/L | 40002/40003 | 1, 66            |
//! | model 103 ID/L | 40070/40071 | 103, 50        |
//! | W            | 40084       | 31250            |
//! | W_SF         | 40085       | 2 → W = 3.125 MW |

use std::collections::HashMap;

/// Holding registers (FC3). Read-only device.
pub fn holding_registers() -> HashMap<u16, u16> {
    HashMap::from([
        (40000, 0x5375),
        (40001, 0x6E53),
        (40002, 1),
        (40003, 66),
        (40070, 103),
        (40071, 50),
        (40084, 31250),
        (40085, 2),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_103_active_power_decodes_with_its_scale_factor() {
        // Arrange
        let m = holding_registers();
        // Act — int16 W × 10^(int16 W_SF), as the gateway applies it
        let w = f64::from(m[&40084] as i16) * 10f64.powi(i32::from(m[&40085] as i16));
        // Assert
        assert_eq!(w, 3_125_000.0);
    }

    #[test]
    fn the_sunspec_map_is_discoverable() {
        // A SunSpec client finds the device by the marker and walks the models
        let m = holding_registers();
        assert_eq!((m[&40000], m[&40001]), (0x5375, 0x6E53));
        assert_eq!((m[&40002], m[&40003]), (1, 66));
        assert_eq!((m[&40070], m[&40071]), (103, 50));
    }
}
