//! Unit tests for the POI meter's active power.

use super::{poi_active_power, write_poi_power};
use std::collections::{HashMap, HashSet};

/// ION9000 active power: float32, big-endian words at 3060-3061.
fn read_f32(holding: &HashMap<u16, u16>) -> f32 {
    f32::from_bits((u32::from(holding[&3060]) << 16) | u32::from(holding[&3061]))
}

#[test]
fn poi_imports_site_load_minus_battery_discharge() {
    // 72.8 kW load, racks discharging 20 + 10 kW -> POI imports 42.8 kW
    assert_eq!(poi_active_power(72_800.0, &[20_000.0, 10_000.0]), 42_800.0);
}

#[test]
fn charging_racks_add_to_poi_import() {
    assert_eq!(poi_active_power(72_800.0, &[-20_000.0]), 92_800.0);
}

#[test]
fn discharge_beyond_load_is_negative_export() {
    assert_eq!(poi_active_power(72_800.0, &[50_000.0, 30_000.0]), -7_200.0);
}

#[test]
fn register_holds_negative_export_as_float32() {
    // Arrange
    let mut holding = HashMap::new();
    // Act
    write_poi_power(&mut holding, &HashSet::new(), -7_200.5);
    // Assert — decodes back through float32 high_low
    assert_eq!(read_f32(&holding), -7_200.5);
}

#[test]
fn a_driven_poi_register_is_left_alone() {
    let driven_bits = 99.0_f32.to_bits();
    let mut holding = HashMap::from([
        (3060, (driven_bits >> 16) as u16),
        (3061, driven_bits as u16),
    ]);
    write_poi_power(&mut holding, &HashSet::from([3061]), 42_800.0);
    assert_eq!(read_f32(&holding), 99.0);
}
