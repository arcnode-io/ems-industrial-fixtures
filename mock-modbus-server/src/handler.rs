//! RequestHandler impl backed by a register map mutated by `Simulator`.

use rodbus::ExceptionCode;
use rodbus::server::RequestHandler;
use std::collections::{HashMap, HashSet};

/// Handles read_holding_register against the live register map.
/// `holding` is `pub` so the simulator tick task can update values in place
/// while the rodbus server reads them concurrently (synchronized via the
/// `Arc<Mutex<...>>` rodbus's `wrap()` provides).
pub struct MeterHandler {
    /// Live register values keyed by Modbus address. Mutated by Simulator::tick.
    pub holding: HashMap<u16, u16>,
    /// Addresses owned by the external control surface (digital-twin).
    /// The simulator skips channels touching these so a driven value
    /// survives past the next tick.
    pub driven: HashSet<u16>,
}

impl MeterHandler {
    /// Build a handler with the initial register map.
    pub fn new(holding: HashMap<u16, u16>) -> Self {
        Self {
            holding,
            driven: HashSet::new(),
        }
    }

    /// Apply a batch of external register writes atomically (caller holds
    /// the lock) and mark each address as control-driven.
    pub fn apply_writes(&mut self, registers: &HashMap<u16, u16>) {
        for (&address, &value) in registers {
            self.holding.insert(address, value);
            self.driven.insert(address);
        }
    }
}

impl RequestHandler for MeterHandler {
    fn read_holding_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        self.holding
            .get(&address)
            .copied()
            .ok_or(ExceptionCode::IllegalDataAddress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_writes_updates_values_and_marks_driven() {
        // Arrange
        let mut handler = MeterHandler::new(HashMap::from([(4000, 1)]));
        let writes = HashMap::from([(4000, 15), (4001, 16960)]);

        // Act
        handler.apply_writes(&writes);

        // Assert
        assert_eq!(handler.holding.get(&4000), Some(&15));
        assert_eq!(handler.holding.get(&4001), Some(&16960));
        assert!(handler.driven.contains(&4000));
        assert!(handler.driven.contains(&4001));
    }
}
