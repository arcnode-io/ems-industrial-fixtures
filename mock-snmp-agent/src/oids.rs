//! Canned OID map for the revenue_meter template.
//!
//! Uses a fake private enterprise number (41999 — not assigned) under
//! `1.3.6.1.4.1.<enterprise>.1.1.0` for kwh_delivered. Integer Wh, sawtooth
//! 1_000_000 → 1_010_000.

use std::collections::HashMap;

/// kwh_delivered OID components (1.3.6.1.4.1.41999.1.1.0).
pub const OID_KWH_DELIVERED: &[u32] = &[1, 3, 6, 1, 4, 1, 41999, 1, 1, 0];

/// Build the initial OID → integer-value map.
pub fn initial_values() -> HashMap<Vec<u32>, i64> {
    let mut m = HashMap::new();
    m.insert(OID_KWH_DELIVERED.to_vec(), 1_000_000);
    m
}
