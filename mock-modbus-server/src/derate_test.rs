//! The SoC derate curve: rated in the middle, linear to 0 W at the ends.

use super::{max_charge_w, max_discharge_w};

const RATED: f64 = 4_000_000.0;

#[test]
fn rated_through_the_middle_of_the_range() {
    assert_eq!(max_discharge_w(RATED, 50.0), RATED);
    assert_eq!(max_charge_w(RATED, 50.0), RATED);
    assert_eq!(max_discharge_w(RATED, 20.0), RATED);
    assert_eq!(max_charge_w(RATED, 90.0), RATED);
}

#[test]
fn discharge_tapers_to_zero_at_empty() {
    assert_eq!(max_discharge_w(RATED, 10.0), 2_000_000.0);
    assert_eq!(max_discharge_w(RATED, 0.0), 0.0);
    // the charge side is unaffected near empty
    assert_eq!(max_charge_w(RATED, 0.0), RATED);
}

#[test]
fn charge_tapers_to_zero_at_full() {
    assert_eq!(max_charge_w(RATED, 95.0), 2_000_000.0);
    assert_eq!(max_charge_w(RATED, 100.0), 0.0);
    assert_eq!(max_discharge_w(RATED, 100.0), RATED);
}
