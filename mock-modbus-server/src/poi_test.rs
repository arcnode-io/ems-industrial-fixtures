//! Unit tests for the POI meter's active power.

use super::{poi_active_power, write_poi_power};
use std::collections::{HashMap, HashSet};

fn read_i32(holding: &HashMap<u16, u16>) -> i32 {
    ((u32::from(holding[&4030]) << 16) | u32::from(holding[&4031])) as i32
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
fn register_holds_negative_export_as_twos_complement() {
    // Arrange
    let mut holding = HashMap::new();
    // Act
    write_poi_power(&mut holding, &HashSet::new(), -7_200.0);
    // Assert — decodes back through int32 high_low
    assert_eq!(read_i32(&holding), -7_200);
}

#[test]
fn a_driven_poi_register_is_left_alone() {
    let mut holding = HashMap::from([(4030, 0), (4031, 99)]);
    write_poi_power(&mut holding, &HashSet::from([4031]), 42_800.0);
    assert_eq!(read_i32(&holding), 99);
}
