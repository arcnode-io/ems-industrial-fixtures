//! RequestHandler impl backed by a static register map.

use rodbus::server::RequestHandler;
use rodbus::ExceptionCode;
use std::collections::HashMap;

/// Handles read_holding_register against a sparse register map.
pub struct MeterHandler {
    /// Canned register values keyed by Modbus address.
    holding: HashMap<u16, u16>,
}

impl MeterHandler {
    /// Build a handler from a pre-populated register map.
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
