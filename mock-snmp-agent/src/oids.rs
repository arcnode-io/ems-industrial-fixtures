//! Canned OID map for the `pdu` template (Server Tech PRO3X intelligent PDU).
//!
//! Mirrors the OIDs the template binds (Server Tech enterprise 1718).
//! For Tier 1, only `input_current` is exposed; sawtooth 100 → 200 (deci-amps).

use std::collections::HashMap;

/// input_current OID components (1.3.6.1.4.1.1718.4.1.3.3.1.7).
pub const OID_INPUT_CURRENT: &[u32] = &[1, 3, 6, 1, 4, 1, 1718, 4, 1, 3, 3, 1, 7];

/// Build the initial OID → integer-value map.
pub fn initial_values() -> HashMap<Vec<u32>, i64> {
    let mut m = HashMap::new();
    m.insert(OID_INPUT_CURRENT.to_vec(), 100);
    m
}
