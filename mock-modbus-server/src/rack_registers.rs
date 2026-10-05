//! bess_rack register map (per edp-api bess_rack.yaml; addresses are
//! placeholders until commissioning) and its int32 word helpers.
//!
//! | measurement         | addr  | type             |
//! |---------------------|-------|------------------|
//! | state_of_charge     | 0     | uint16, 0.1 %    |
//! | active_power        | 10-11 | int32, W         |
//! | reactive_power      | 12-13 | int32, var       |
//! | voltage             | 20    | uint16, 0.1 V    |
//! | frequency           | 22    | uint16, 0.01 Hz  |
//! | energy_discharged   | 30-31 | int32, Wh        |
//! | operating_state     | 40    | uint16 enum      |
//! | set_active_power    | 50-51 | int32, W (cmd)   |
//! | max_charge_power    | 60-61 | int32, W (+)     |
//! | max_discharge_power | 62-63 | int32, W (+)     |

use crate::derate;
use std::collections::{HashMap, HashSet};

/// state_of_charge register (uint16, scale 0.1), per bess_rack.yaml.
pub const SOC: u16 = 0;
/// active_power register pair (int32 high_low).
pub const ACTIVE_POWER: u16 = 10;
/// energy_discharged register pair (int32 high_low, Wh).
pub const ENERGY_DISCHARGED: u16 = 30;
/// operating_state register.
pub const OPERATING_STATE: u16 = 40;
/// set_active_power command register pair (int32 high_low).
pub const SETPOINT: u16 = 50;
/// max_charge_power register pair (int32 high_low, W magnitude).
pub const MAX_CHARGE: u16 = 60;
/// max_discharge_power register pair (int32 high_low, W magnitude).
pub const MAX_DISCHARGE: u16 = 62;

/// operating_state STANDBY.
pub const STANDBY: u16 = 0;
/// operating_state CHARGING.
pub const CHARGING: u16 = 1;
/// operating_state DISCHARGING.
pub const DISCHARGING: u16 = 2;

/// Initial register map for a rack at `soc_percent`: 480.0 V, 60.00 Hz,
/// idle, no reactive power, limits derated from `rated_w` at that SoC.
pub fn initial_registers(soc_percent: f64, rated_w: f64) -> HashMap<u16, u16> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let soc = (soc_percent * 10.0).round() as u16;
    let mut holding = HashMap::from([
        (SOC, soc),
        (10, 0),
        (11, 0),
        (12, 0),
        (13, 0),
        (20, 4800),
        (22, 6000),
        (30, 0),
        (31, 0),
        (OPERATING_STATE, STANDBY),
        (SETPOINT, 0),
        (SETPOINT + 1, 0),
    ]);
    let none = HashSet::new();
    let charge = derate::max_charge_w(rated_w, soc_percent);
    let discharge = derate::max_discharge_w(rated_w, soc_percent);
    #[allow(clippy::cast_possible_truncation)]
    {
        put_i32(&mut holding, &none, MAX_CHARGE, charge.round() as i32);
        put_i32(&mut holding, &none, MAX_DISCHARGE, discharge.round() as i32);
    }
    holding
}

/// Read an int32 (high word first) from two consecutive registers.
pub fn read_i32(holding: &HashMap<u16, u16>, addr: u16) -> i32 {
    let high = u32::from(*holding.get(&addr).unwrap_or(&0));
    let low = u32::from(*holding.get(&(addr + 1)).unwrap_or(&0));
    ((high << 16) | low) as i32
}

/// Write a register unless the control surface is driving it.
pub fn put(holding: &mut HashMap<u16, u16>, driven: &HashSet<u16>, addr: u16, value: u16) {
    if !driven.contains(&addr) {
        holding.insert(addr, value);
    }
}

/// Write an int32 (high word first) unless either word is driven.
pub fn put_i32(holding: &mut HashMap<u16, u16>, driven: &HashSet<u16>, addr: u16, value: i32) {
    if driven.contains(&addr) || driven.contains(&(addr + 1)) {
        return;
    }
    let raw = value as u32;
    holding.insert(addr, (raw >> 16) as u16);
    holding.insert(addr + 1, raw as u16);
}
