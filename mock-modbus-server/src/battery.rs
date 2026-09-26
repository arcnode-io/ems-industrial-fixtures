//! Simulated bess_rack battery (`MODBUS_PROFILE=bess_rack`).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Static rack parameters.
pub struct BatteryConfig {
    /// Usable energy capacity.
    pub capacity_kwh: f64,
    /// Nameplate charge/discharge limit (symmetric).
    pub power_limit_w: f64,
    /// Simulated seconds per wall-clock second.
    pub time_scale: f64,
}

/// Live rack state.
pub struct Battery {
    /// Static rack parameters.
    config: BatteryConfig,
    /// Full-precision SoC; the register only holds it to 0.1%.
    soc_percent: f64,
    /// Lifetime discharged energy, full precision.
    energy_discharged_wh: f64,
}

impl Battery {
    /// Build a rack starting at `initial_soc_percent`.
    pub fn new(config: BatteryConfig, initial_soc_percent: f64) -> Self {
        Self {
            config,
            soc_percent: initial_soc_percent,
            energy_discharged_wh: 0.0,
        }
    }

    /// Advance the model by `dt` of wall time and write its registers.
    ///
    /// Reason: SoC lives here as f64, not re-read from its register. The
    /// register's 0.1% step is 4 kWh; a single tick at demo power moves far
    /// less, so round-tripping through it would round every tick away. A SoC
    /// driven through the control surface is still taken as the new truth.
    pub fn step(&mut self, holding: &mut HashMap<u16, u16>, driven: &HashSet<u16>, dt: Duration) {
        if driven.contains(&SOC) {
            self.soc_percent = f64::from(holding[&SOC]) * 0.1;
        }
        let power = self.deliverable(f64::from(read_i32(holding, SETPOINT)));
        let sim_hours = dt.as_secs_f64() * self.config.time_scale / 3600.0;
        let energy_kwh = power * sim_hours / 1000.0;
        self.soc_percent =
            (self.soc_percent - energy_kwh / self.config.capacity_kwh * 100.0).clamp(0.0, 100.0);
        if power > 0.0 {
            self.energy_discharged_wh += power * sim_hours;
        }

        let state = if power > 0.0 {
            DISCHARGING
        } else if power < 0.0 {
            CHARGING
        } else {
            STANDBY
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            put(
                holding,
                driven,
                SOC,
                (self.soc_percent * 10.0).round() as u16,
            );
            put_i32(holding, driven, ACTIVE_POWER, power.round() as i32);
            put_i32(
                holding,
                driven,
                ENERGY_DISCHARGED,
                self.energy_discharged_wh.round() as i32,
            );
        }
        put(holding, driven, OPERATING_STATE, state);
    }

    /// The setpoint the rack can actually deliver: clamped to nameplate, and
    /// zero when it would discharge an empty rack or charge a full one.
    fn deliverable(&self, setpoint_w: f64) -> f64 {
        let limit = self.config.power_limit_w;
        let power = setpoint_w.clamp(-limit, limit);
        let empty = power > 0.0 && self.soc_percent <= 0.0;
        let full = power < 0.0 && self.soc_percent >= 100.0;
        if empty || full { 0.0 } else { power }
    }
}

/// Tesla Megapack 2 XL capacity, per edp-api bess_rack.yaml `capacity_kwh`.
const RACK_CAPACITY_KWH: f64 = 4000.0;
/// Per edp-api bess_rack.yaml `active_power.bounds` (±4 MW).
const RACK_POWER_LIMIT_W: f64 = 4_000_000.0;
/// Default initial SoC, well above any plausible reserve floor.
const DEFAULT_INITIAL_SOC_PERCENT: f64 = 60.0;

/// Build a rack and its initial registers from env: `BATTERY_TIME_SCALE`
/// (default 1.0; e.g. 60 makes a 200 kW drain visible in minutes) and
/// `BATTERY_INITIAL_SOC_PERCENT` (default 60).
pub fn from_env() -> Result<(Battery, HashMap<u16, u16>), String> {
    let time_scale = env_f64("BATTERY_TIME_SCALE", 1.0)?;
    let soc = env_f64("BATTERY_INITIAL_SOC_PERCENT", DEFAULT_INITIAL_SOC_PERCENT)?;
    if !(0.0..=100.0).contains(&soc) {
        return Err(format!(
            "BATTERY_INITIAL_SOC_PERCENT must be 0-100, got {soc}"
        ));
    }
    if time_scale <= 0.0 {
        return Err(format!("BATTERY_TIME_SCALE must be > 0, got {time_scale}"));
    }
    let config = BatteryConfig {
        capacity_kwh: RACK_CAPACITY_KWH,
        power_limit_w: RACK_POWER_LIMIT_W,
        time_scale,
    };
    Ok((Battery::new(config, soc), initial_registers(soc)))
}

/// Parse an optional f64 env var, falling back to `default` when unset.
fn env_f64(name: &str, default: f64) -> Result<f64, String> {
    match std::env::var(name) {
        Ok(raw) => raw
            .parse()
            .map_err(|_| format!("{name} is not a number: {raw}")),
        Err(_) => Ok(default),
    }
}

/// state_of_charge register (uint16, scale 0.1), per bess_rack.yaml.
const SOC: u16 = 0;
/// active_power register pair (int32 high_low).
const ACTIVE_POWER: u16 = 10;
/// energy_discharged register pair (int32 high_low, Wh).
const ENERGY_DISCHARGED: u16 = 30;
/// operating_state register.
const OPERATING_STATE: u16 = 40;
/// set_active_power command register pair (int32 high_low).
const SETPOINT: u16 = 50;

/// operating_state STANDBY.
const STANDBY: u16 = 0;
/// operating_state CHARGING.
const CHARGING: u16 = 1;
/// operating_state DISCHARGING.
const DISCHARGING: u16 = 2;

/// Initial register map for a rack at `soc_percent`: 480.0 V, 60.00 Hz,
/// idle, no reactive power.
pub fn initial_registers(soc_percent: f64) -> HashMap<u16, u16> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let soc = (soc_percent * 10.0).round() as u16;
    HashMap::from([
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
    ])
}

/// Read an int32 (high word first) from two consecutive registers.
fn read_i32(holding: &HashMap<u16, u16>, addr: u16) -> i32 {
    let high = u32::from(*holding.get(&addr).unwrap_or(&0));
    let low = u32::from(*holding.get(&(addr + 1)).unwrap_or(&0));
    ((high << 16) | low) as i32
}

/// Write a register unless the control surface is driving it.
fn put(holding: &mut HashMap<u16, u16>, driven: &HashSet<u16>, addr: u16, value: u16) {
    if !driven.contains(&addr) {
        holding.insert(addr, value);
    }
}

/// Write an int32 (high word first) unless either word is driven.
fn put_i32(holding: &mut HashMap<u16, u16>, driven: &HashSet<u16>, addr: u16, value: i32) {
    if driven.contains(&addr) || driven.contains(&(addr + 1)) {
        return;
    }
    let raw = value as u32;
    holding.insert(addr, (raw >> 16) as u16);
    holding.insert(addr + 1, raw as u16);
}

#[cfg(test)]
#[path = "battery_test.rs"]
mod tests;
