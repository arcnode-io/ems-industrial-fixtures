//! Data simulator: mutates the handler's register map on a tick.
//!
//! Per-channel strategies describe the range + step. Currently one strategy:
//! `Int32SawtoothSim` increments a two-word holding-register pair between
//! `min` and `max` by `step`, wrapping when it overflows the max.
//! Channels whose addresses are control-driven (set via the HTTP control
//! surface) are skipped so external values survive across ticks.

use std::collections::{HashMap, HashSet};

/// Sawtooth simulator for a single int32-shaped channel across two consecutive
/// holding registers (high word at `addr_high`, low word at `addr_low`).
pub struct Int32SawtoothSim {
    /// Address of the high word.
    pub addr_high: u16,
    /// Address of the low word.
    pub addr_low: u16,
    /// Lower bound of the sawtooth (inclusive).
    pub min: u32,
    /// Upper bound (wraps back to `min` when exceeded).
    pub max: u32,
    /// Increment applied each tick.
    pub step: u32,
}

/// Drives the holding-register map. Add channels by extending `int32_saws`.
pub struct Simulator {
    /// All int32-shaped channels under simulation.
    int32_saws: Vec<Int32SawtoothSim>,
}

impl Simulator {
    /// Build the simulator with the canonical revenue_meter channels.
    pub fn new() -> Self {
        Self {
            int32_saws: vec![Int32SawtoothSim {
                addr_high: 4000,
                addr_low: 4001,
                min: 1_000_000,
                max: 1_010_000,
                step: 100,
            }],
        }
    }

    /// Advance every non-driven channel by one step. Wraps the value to
    /// `min` when it would exceed `max`. Channels with either word under
    /// external control are left untouched.
    pub fn tick(&self, holding: &mut HashMap<u16, u16>, driven: &HashSet<u16>) {
        for sim in &self.int32_saws {
            if driven.contains(&sim.addr_high) || driven.contains(&sim.addr_low) {
                continue;
            }
            let high = *holding.get(&sim.addr_high).unwrap_or(&0);
            let low = *holding.get(&sim.addr_low).unwrap_or(&0);
            let mut value = ((high as u32) << 16) | (low as u32);
            value = value.saturating_add(sim.step);
            if value > sim.max {
                value = sim.min;
            }
            holding.insert(sim.addr_high, (value >> 16) as u16);
            holding.insert(sim.addr_low, value as u16);
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
    fn tick_advances_undriven_sawtooth() {
        // Arrange — value 1_000_000 = 0x000F4240
        let mut holding = HashMap::from([(4000, 0x000F), (4001, 0x4240)]);
        let sim = Simulator::new();

        // Act
        sim.tick(&mut holding, &HashSet::new());

        // Assert — advanced by step 100 -> 1_000_100 = 0x000F42A4
        assert_eq!(holding.get(&4000), Some(&0x000F));
        assert_eq!(holding.get(&4001), Some(&0x42A4));
    }

    #[test]
    fn tick_skips_control_driven_channels() {
        // Arrange — 4000/4001 externally driven to an arbitrary value
        let mut holding = HashMap::from([(4000, 0x1234), (4001, 0x5678)]);
        let driven = HashSet::from([4000]);
        let sim = Simulator::new();

        // Act
        sim.tick(&mut holding, &driven);

        // Assert — untouched
        assert_eq!(holding.get(&4000), Some(&0x1234));
        assert_eq!(holding.get(&4001), Some(&0x5678));
    }
}
