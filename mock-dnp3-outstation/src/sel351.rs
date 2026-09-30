//! SEL-351 protective relay points, from SEL's DNP3 device profile
//! (dnpDP-351R100.xml), per the power-engineer's map for edp-api's
//! protective_relay template. Analogs are served as 32-bit floats (Var 5)
//! in the relay's primary units: amps, and kilovolts phase-to-neutral.
//!
//! | measurement           | point | value      |
//! |-----------------------|-------|------------|
//! | phase_a/b/c_current   | AI 0/2/4   | 150 / 152 / 148 A |
//! | phase_voltage_a/b/c   | AI 8/10/12 | 7.97 / 7.95 / 7.99 kV (13.8 kV L-L) |
//! | trip_status           | BI 9  | clear (TRIP_LED) |
//! | ground_fault          | BI 15 | clear (G target) |
//! | anti_islanding_armed  | BI 24 | set (user-settable slot) |
//! | ride_through_enabled  | BI 25 | set (user-settable slot) |
//!
//! Values differ per phase so a point mixup shows as a wrong number.

use dnp3::app::measurement::{AnalogInput, BinaryInput, Flags, Time};
use dnp3::outstation::database::{
    Add, AnalogInputConfig, BinaryInputConfig, Database, EventAnalogInputVariation,
    EventBinaryInputVariation, EventClass, StaticAnalogInputVariation, StaticBinaryInputVariation,
    Update, UpdateOptions,
};

/// Analog inputs: (point index, value in the relay's primary units).
const ANALOGS: [(u16, f64); 6] = [
    (0, 150.0),
    (2, 152.0),
    (4, 148.0),
    (8, 7.97),
    (10, 7.95),
    (12, 7.99),
];

/// Binary inputs: (point index, set).
const BINARIES: [(u16, bool); 4] = [(9, false), (15, false), (24, true), (25, true)];

/// Seed every SEL-351 point with its static value.
pub fn seed(db: &mut Database) {
    let now = Time::synchronized(0);
    for (index, value) in ANALOGS {
        let config = AnalogInputConfig {
            s_var: StaticAnalogInputVariation::Group30Var5,
            e_var: EventAnalogInputVariation::Group32Var5,
            deadband: 0.0,
        };
        db.add(index, Some(EventClass::Class1), config);
        db.update(
            index,
            &AnalogInput::new(value, Flags::ONLINE, now),
            UpdateOptions::default(),
        );
    }
    for (index, value) in BINARIES {
        let config = BinaryInputConfig {
            s_var: StaticBinaryInputVariation::Group1Var2,
            e_var: EventBinaryInputVariation::Group2Var1,
        };
        db.add(index, Some(EventClass::Class1), config);
        db.update(
            index,
            &BinaryInput::new(value, Flags::ONLINE, now),
            UpdateOptions::default(),
        );
    }
}
