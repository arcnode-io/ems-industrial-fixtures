//! Sawtooth strategy for DNP3 analog input points.
//!
//! `protective_relay` template binds `phase_a_current` to analog_input
//! point_index=0. Sawtooth 100 → 200 amps (deci-amps).
//! Points under external control (HTTP control surface) are skipped so
//! driven values survive across ticks.

use dnp3::app::Timestamp;
use dnp3::app::measurement::{AnalogInput, Flags, Time};
use dnp3::outstation::OutstationHandle;
use dnp3::outstation::database::{Update, UpdateOptions};
use std::collections::HashSet;
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

    /// Advance every non-driven channel by one step, returning the
    /// (index, value) updates to apply. Pure — testable without an
    /// outstation.
    fn advance(&mut self, driven: &HashSet<u16>) -> Vec<(u16, f64)> {
        let mut updates = Vec::new();
        for sim in &mut self.analogs {
            if driven.contains(&sim.index) {
                continue;
            }
            sim.cur += sim.step;
            if sim.cur > sim.max {
                sim.cur = sim.min;
            }
            updates.push((sim.index, sim.cur));
        }
        updates
    }

    /// Advance and write back into the outstation database via a transaction.
    pub fn tick(&mut self, outstation: &OutstationHandle, driven: &HashSet<u16>) {
        for (index, value) in self.advance(driven) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_moves_undriven_sawtooth() {
        // Arrange
        let mut sim = Simulator::new();

        // Act
        let updates = sim.advance(&HashSet::new());

        // Assert — 100 + 5
        assert_eq!(updates, vec![(0, 105.0)]);
    }

    #[test]
    fn advance_skips_control_driven_points() {
        // Arrange
        let mut sim = Simulator::new();
        let driven = HashSet::from([0]);

        // Act
        let updates = sim.advance(&driven);

        // Assert — nothing to write, internal state untouched
        assert_eq!(updates, vec![]);
        assert_eq!(sim.advance(&HashSet::new()), vec![(0, 105.0)]);
    }
}
