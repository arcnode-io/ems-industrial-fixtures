//! Unit tests for the simulated bess_rack battery.

use super::{Battery, BatteryConfig};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

fn config(time_scale: f64) -> BatteryConfig {
    BatteryConfig {
        capacity_kwh: 4000.0,
        power_limit_w: 4_000_000.0,
        time_scale,
    }
}

/// Holding map with a commanded setpoint in 50-51 (int32, high word first).
fn commanded(setpoint_w: i32) -> HashMap<u16, u16> {
    let raw = setpoint_w as u32;
    HashMap::from([(50, (raw >> 16) as u16), (51, raw as u16)])
}

fn active_power(holding: &HashMap<u16, u16>) -> i32 {
    ((u32::from(holding[&10]) << 16) | u32::from(holding[&11])) as i32
}

const ONE_HOUR: Duration = Duration::from_secs(3600);

#[test]
fn discharging_follows_setpoint_and_drains_soc() {
    // Arrange — 400 kW for one hour = 400 kWh = 10% of 4000 kWh
    let mut battery = Battery::new(config(1.0), 50.0);
    let mut holding = commanded(400_000);
    // Act
    battery.step(&mut holding, &HashSet::new(), ONE_HOUR);
    // Assert
    assert_eq!(active_power(&holding), 400_000);
    assert_eq!(holding[&0], 400); // 40.0%
    assert_eq!(holding[&40], 2); // DISCHARGING
}

#[test]
fn charging_fills_soc() {
    let mut battery = Battery::new(config(1.0), 50.0);
    let mut holding = commanded(-400_000);
    battery.step(&mut holding, &HashSet::new(), ONE_HOUR);
    assert_eq!(active_power(&holding), -400_000);
    assert_eq!(holding[&0], 600); // 60.0%
    assert_eq!(holding[&40], 1); // CHARGING
}

#[test]
fn time_scale_compresses_the_same_energy_into_less_wall_time() {
    // Arrange — 60x: one minute of wall time behaves like one hour
    let mut battery = Battery::new(config(60.0), 50.0);
    let mut holding = commanded(400_000);
    // Act
    battery.step(&mut holding, &HashSet::new(), Duration::from_secs(60));
    // Assert
    assert_eq!(holding[&0], 400);
}

#[test]
fn sub_resolution_ticks_still_accumulate() {
    // Arrange — 200 kW for 1 s is ~0.06 kWh, far below the register's 0.1%
    // (4 kWh) step; 3600 such ticks must still add up to 200 kWh = 5%.
    let mut battery = Battery::new(config(1.0), 50.0);
    let mut holding = commanded(200_000);
    // Act
    for _ in 0..3600 {
        battery.step(&mut holding, &HashSet::new(), Duration::from_secs(1));
    }
    // Assert
    assert_eq!(holding[&0], 450);
}

#[test]
fn setpoint_is_clamped_to_the_power_limit() {
    let mut battery = Battery::new(config(1.0), 50.0);
    let mut holding = commanded(10_000_000);
    battery.step(&mut holding, &HashSet::new(), Duration::from_secs(1));
    assert_eq!(active_power(&holding), 4_000_000);
}

#[test]
fn an_empty_battery_cannot_discharge() {
    let mut battery = Battery::new(config(1.0), 0.0);
    let mut holding = commanded(100_000);
    battery.step(&mut holding, &HashSet::new(), ONE_HOUR);
    assert_eq!(active_power(&holding), 0);
    assert_eq!(holding[&0], 0);
    assert_eq!(holding[&40], 0); // STANDBY
}

#[test]
fn a_full_battery_cannot_charge() {
    let mut battery = Battery::new(config(1.0), 100.0);
    let mut holding = commanded(-100_000);
    battery.step(&mut holding, &HashSet::new(), ONE_HOUR);
    assert_eq!(active_power(&holding), 0);
    assert_eq!(holding[&0], 1000);
}

#[test]
fn soc_set_through_the_control_surface_becomes_the_new_truth() {
    // Arrange — a test drives SoC to 25.0% via PUT /registers
    let mut battery = Battery::new(config(1.0), 50.0);
    let mut holding = commanded(400_000);
    holding.insert(0, 250);
    let driven = HashSet::from([0]);
    // Act
    battery.step(&mut holding, &driven, ONE_HOUR);
    // Assert — driven register left as-is, but the model restarted from it
    assert_eq!(holding[&0], 250);
    battery.step(&mut holding, &HashSet::new(), ONE_HOUR);
    assert_eq!(holding[&0], 50); // 25% - 10% (tick 1) - 10% (tick 2)
}

#[test]
fn discharge_accumulates_energy_discharged() {
    let mut battery = Battery::new(config(1.0), 50.0);
    let mut holding = commanded(400_000);
    battery.step(&mut holding, &HashSet::new(), ONE_HOUR);
    let wh = (u32::from(holding[&30]) << 16) | u32::from(holding[&31]);
    assert_eq!(wh, 400_000);
}
