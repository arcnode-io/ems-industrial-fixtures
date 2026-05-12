//! Mock BACnet/IP device exposing a few `AnalogInput` objects with sawtooth
//! values. Handles `Who-Is` and `ReadProperty` over UDP 47808. Used by the
//! gateway e2e to validate the BACnet protocol path.

pub mod simulator;

/// Default BACnet/IP port per ASHRAE 135 Annex J.
pub const DEFAULT_PORT: u16 = 47808;
/// Default device instance number reported by this mock.
pub const DEFAULT_DEVICE_INSTANCE: u32 = 11001;
