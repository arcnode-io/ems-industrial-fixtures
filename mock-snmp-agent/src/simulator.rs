//! Data simulator: mutates the OID value map on each tick.
//! OIDs under external control (HTTP control surface) are skipped so
//! driven values survive across ticks.

use crate::oids::OID_INPUT_CURRENT;
use std::collections::{HashMap, HashSet};

/// Sawtooth strategy for a single OID with an integer value.
struct IntSawtooth {
    /// Target OID component vector.
    oid: Vec<u32>,
    /// Inclusive lower bound.
    min: i64,
    /// Wraps back to `min` when exceeded.
    max: i64,
    /// Increment per tick.
    step: i64,
}

/// Drives the OID value map. Add OIDs by extending `saws`.
pub struct Simulator {
    /// All integer-shaped OIDs under simulation.
    saws: Vec<IntSawtooth>,
}

impl Simulator {
    /// Build with the canonical revenue_meter OIDs.
    pub fn new() -> Self {
        Self {
            saws: vec![IntSawtooth {
                oid: OID_INPUT_CURRENT.to_vec(),
                min: 100,
                max: 200,
                step: 5,
            }],
        }
    }

    /// Advance every non-driven OID by one step. Wraps to `min` past `max`.
    pub fn tick(&self, values: &mut HashMap<Vec<u32>, i64>, driven: &HashSet<Vec<u32>>) {
        for sim in &self.saws {
            if driven.contains(&sim.oid) {
                continue;
            }
            let cur = *values.get(&sim.oid).unwrap_or(&sim.min);
            let next = if cur + sim.step > sim.max {
                sim.min
            } else {
                cur + sim.step
            };
            values.insert(sim.oid.clone(), next);
        }
    }
}

impl Default for Simulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_advances_undriven_oid() {
        // Arrange
        let mut values = HashMap::from([(OID_INPUT_CURRENT.to_vec(), 100)]);
        let sim = Simulator::new();

        // Act
        sim.tick(&mut values, &HashSet::new());

        // Assert
        assert_eq!(values.get(OID_INPUT_CURRENT), Some(&105));
    }

    #[test]
    fn tick_skips_control_driven_oid() {
        // Arrange — externally driven value
        let mut values = HashMap::from([(OID_INPUT_CURRENT.to_vec(), 42)]);
        let driven = HashSet::from([OID_INPUT_CURRENT.to_vec()]);
        let sim = Simulator::new();

        // Act
        sim.tick(&mut values, &driven);

        // Assert — untouched
        assert_eq!(values.get(OID_INPUT_CURRENT), Some(&42));
    }
}
