//! RequestHandler impl backed by a register map mutated by `Simulator`.

use rodbus::server::RequestHandler;
use rodbus::ExceptionCode;
use std::collections::HashMap;

/// Handles read_holding_register against the live register map.
/// `holding` is `pub` so the simulator tick task can update values in place
/// while the rodbus server reads them concurrently (synchronized via the
/// `Arc<Mutex<...>>` rodbus's `wrap()` provides).
pub struct MeterHandler {
    /// Live register values keyed by Modbus address. Mutated by Simulator::tick.
    pub holding: HashMap<u16, u16>,
}

impl MeterHandler {
    /// Build a handler with the initial register map.
    pub fn new(holding: HashMap<u16, u16>) -> Self {
        Self { holding }
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
