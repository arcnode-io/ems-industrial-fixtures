//! Canned holding-register map for the revenue_meter template.
//!
//! kwh_delivered: int32 at addr 4000-4001, word_order high_low, scale 1.0.
//! Value chosen so a successful e2e read yields exactly 1_000_000 Wh.
//!
//!   int32 1_000_000 = 0x000F4240
//!     holding[4000] = 0x000F (high word, 15)
//!     holding[4001] = 0x4240 (low word,  16960)

use std::collections::HashMap;

/// Build the canned holding-register map.
pub fn holding_registers() -> HashMap<u16, u16> {
    let mut m = HashMap::new();
    m.insert(4000, 0x000F);
    m.insert(4001, 0x4240);
    m
}
