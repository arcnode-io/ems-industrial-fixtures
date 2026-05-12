//! Data simulator: mutates the in-memory Thermal resource on each tick.
//!
//! The `network_switch` template reads `/Chassis/SW1/Thermal` and points at
//! `/Temperatures/0/ReadingCelsius` (inlet) and `/Temperatures/1/ReadingCelsius`
//! (ASIC) and `/Fans/0/Reading` (fan speed). Each follows its own sawtooth.

/// Sawtooth strategy for a single float-valued field.
struct FloatSawtooth {
    /// Inclusive lower bound.
    min: f64,
    /// Wraps back to `min` when exceeded.
    max: f64,
    /// Increment per tick.
    step: f64,
}

/// Live Thermal resource. Field names mirror the Redfish DSP0266 schema.
pub struct Thermal {
    /// First temperature reading (inlet) in °C.
    pub inlet_temp: f64,
    /// Second temperature reading (ASIC) in °C.
    pub asic_temp: f64,
    /// First fan speed (percent).
    pub fan_speed: f64,
}

impl Thermal {
    /// Build with initial values at each sawtooth's `min`.
    pub fn new() -> Self {
        Self {
            inlet_temp: 20.0,
            asic_temp: 40.0,
            fan_speed: 30.0,
        }
    }
}

impl Default for Thermal {
    fn default() -> Self {
        Self::new()
    }
}

/// Drives the Thermal resource. Add channels by extending `apply`.
pub struct Simulator {
    /// Inlet temperature strategy.
    inlet: FloatSawtooth,
    /// ASIC temperature strategy.
    asic: FloatSawtooth,
    /// Fan speed strategy.
    fan: FloatSawtooth,
}

impl Simulator {
    /// Build with canonical network_switch ranges.
    pub fn new() -> Self {
        Self {
            inlet: FloatSawtooth {
                min: 20.0,
                max: 30.0,
                step: 0.5,
            },
            asic: FloatSawtooth {
                min: 40.0,
                max: 60.0,
                step: 1.0,
            },
            fan: FloatSawtooth {
                min: 30.0,
                max: 80.0,
                step: 2.0,
            },
        }
    }

    /// Advance every field by one step. Wraps to `min` past `max`.
    pub fn tick(&self, t: &mut Thermal) {
        t.inlet_temp = step(t.inlet_temp, &self.inlet);
        t.asic_temp = step(t.asic_temp, &self.asic);
        t.fan_speed = step(t.fan_speed, &self.fan);
    }
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure sawtooth step: increment by `step`, wrap to `min` when exceeding `max`.
fn step(cur: f64, sim: &FloatSawtooth) -> f64 {
    let next = cur + sim.step;
    if next > sim.max { sim.min } else { next }
}
