//! A rack's present charge/discharge limits (bess_rack `max_charge_power`,
//! `max_discharge_power`): rated through the middle of the SoC range,
//! tapering linearly to 0 W at each end.
//!
//! Reason: a Megapack-class AC rack reports one system limit with the PCS
//! DC-window derate already folded in. Discharge falls off as the DC bus
//! sags near empty; charge falls off in the constant-voltage taper near full.

/// Below this SoC (%), discharge tapers linearly to 0 W at 0%.
pub const DISCHARGE_TAPER_BELOW_PERCENT: f64 = 20.0;
/// Above this SoC (%), charge tapers linearly to 0 W at 100%.
pub const CHARGE_TAPER_ABOVE_PERCENT: f64 = 90.0;

/// Present discharge limit, W (positive magnitude).
pub fn max_discharge_w(rated_w: f64, soc_percent: f64) -> f64 {
    rated_w * (soc_percent / DISCHARGE_TAPER_BELOW_PERCENT).clamp(0.0, 1.0)
}

/// Present charge limit, W (positive magnitude).
pub fn max_charge_w(rated_w: f64, soc_percent: f64) -> f64 {
    let taper_width = 100.0 - CHARGE_TAPER_ABOVE_PERCENT;
    rated_w * ((100.0 - soc_percent) / taper_width).clamp(0.0, 1.0)
}

#[cfg(test)]
#[path = "derate_test.rs"]
mod tests;
