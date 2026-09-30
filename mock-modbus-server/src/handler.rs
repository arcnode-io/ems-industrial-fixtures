//! RequestHandler impl backed by a register map mutated by `Simulator`.

use rodbus::server::{RequestHandler, WriteRegisters};
use rodbus::{ExceptionCode, Indexed};
use std::collections::{HashMap, HashSet};

/// Handles holding (FC3) and input (FC4) register reads against the live
/// register maps, and FC6/FC16 writes when `writable`.
/// `holding` is `pub` so the simulator tick task can update values in place
/// while the rodbus server reads them concurrently (synchronized via the
/// `Arc<Mutex<...>>` rodbus's `wrap()` provides).
pub struct MeterHandler {
    /// Live register values keyed by Modbus address. Mutated by Simulator::tick.
    pub holding: HashMap<u16, u16>,
    /// Input registers (FC4), a separate address space from `holding`.
    /// Empty unless a profile models a device that reports via FC4.
    pub input: HashMap<u16, u16>,
    /// Addresses owned by the external control surface (digital-twin).
    /// The simulator skips channels touching these so a driven value
    /// survives past the next tick.
    pub driven: HashSet<u16>,
    /// Whether Modbus protocol writes (function codes 6 and 16) are accepted.
    /// False by default — real Modbus sessions are read-only unless a
    /// fixture explicitly opts a device in as writable.
    pub writable: bool,
}

impl MeterHandler {
    /// Build a handler with the initial register map. Protocol writes are
    /// rejected until `writable` is set via `MeterHandler { writable: true, .. }`.
    pub fn new(holding: HashMap<u16, u16>) -> Self {
        Self {
            holding,
            input: HashMap::new(),
            driven: HashSet::new(),
            writable: false,
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

    fn read_input_register(&self, address: u16) -> Result<u16, ExceptionCode> {
        self.input
            .get(&address)
            .copied()
            .ok_or(ExceptionCode::IllegalDataAddress)
    }

    /// Function code 6. Same writable gate and driven marking as FC16.
    fn write_single_register(&mut self, value: Indexed<u16>) -> Result<(), ExceptionCode> {
        if !self.writable {
            return Err(ExceptionCode::IllegalFunction);
        }
        self.holding.insert(value.index, value.value);
        self.driven.insert(value.index);
        Ok(())
    }

    /// Function code 16. Rejected with `IllegalFunction` unless `writable`
    /// is set — same exception a real read-only Modbus session returns.
    /// Written addresses become control-driven, same as the HTTP surface.
    fn write_multiple_registers(&mut self, values: WriteRegisters) -> Result<(), ExceptionCode> {
        if !self.writable {
            return Err(ExceptionCode::IllegalFunction);
        }
        for reg in values.iterator {
            self.holding.insert(reg.index, reg.value);
            self.driven.insert(reg.index);
        }
        Ok(())
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

    #[test]
    fn input_registers_are_a_separate_space_from_holding() {
        // Arrange — same address, different value per space
        let mut handler = MeterHandler::new(HashMap::from([(10, 1)]));
        handler.input.insert(10, 2);

        // Act
        let holding = handler.read_holding_register(10);
        let input = handler.read_input_register(10);

        // Assert
        assert_eq!(holding, Ok(1));
        assert_eq!(input, Ok(2));
    }

    #[test]
    fn a_single_register_write_lands_only_when_writable() {
        // Arrange
        let mut read_only = MeterHandler::new(HashMap::new());
        let mut writable = MeterHandler::new(HashMap::new());
        writable.writable = true;
        let value = Indexed::new(30, 215);

        // Act
        let rejected = read_only.write_single_register(value);
        let accepted = writable.write_single_register(value);

        // Assert
        assert_eq!(rejected, Err(ExceptionCode::IllegalFunction));
        assert_eq!(accepted, Ok(()));
        assert_eq!(writable.holding.get(&30), Some(&215));
        assert!(writable.driven.contains(&30));
    }
}
