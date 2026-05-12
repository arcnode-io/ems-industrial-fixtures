//! Sawtooth strategy for DNP3 analog input points.
//!
//! `protective_relay` template binds `phase_a_current` to analog_input
//! point_index=0. Sawtooth 100 → 200 amps (deci-amps).

use dnp3::app::Timestamp;
use dnp3::app::measurement::{AnalogInput, Flags, Time};
use dnp3::outstation::OutstationHandle;
use dnp3::outstation::database::{Update, UpdateOptions};
use std::time::SystemTime;

/// Sawtooth strategy for a single analog input point.
struct AnalogInputSawtooth {
    /// DNP3 point index.
    index: u16,
    /// Inclusive lower bound (engineering units, e.g. amps).
    min: f64,
    /// Wraps back to `min` when exceeded.
    max: f64,
    /// Increment per tick.
    step: f64,
    /// Current value (mutated each tick).
    cur: f64,
}

/// Drives the outstation's analog input points.
pub struct Simulator {
    /// All analog inputs under simulation.
    analogs: Vec<AnalogInputSawtooth>,
}

impl Simulator {
    /// Build with canonical protective_relay channels.
    pub fn new() -> Self {
        Self {
            analogs: vec![AnalogInputSawtooth {
                index: 0,
                min: 100.0,
                max: 200.0,
                step: 5.0,
                cur: 100.0,
            }],
        }
    }

    /// Advance every analog input by one step, write back into the outstation
    /// database via a transaction.
    pub fn tick(&mut self, outstation: &OutstationHandle) {
        for sim in &mut self.analogs {
            sim.cur += sim.step;
            if sim.cur > sim.max {
                sim.cur = sim.min;
            }
            let value = sim.cur;
            let index = sim.index;
            outstation.transaction(|db| {
                db.update(
                    index,
                    &AnalogInput::new(value, Flags::ONLINE, current_time()),
                    UpdateOptions::detect_event(),
                );
            });
        }
    }
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Wall-clock time as a DNP3 Synchronized timestamp.
fn current_time() -> Time {
    let epoch = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch");
    Time::Synchronized(Timestamp::new(epoch.as_millis() as u64))
}
