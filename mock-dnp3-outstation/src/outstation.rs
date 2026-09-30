//! Outstation boilerplate shared by the server and its tests: the no-op
//! application/information/listener, a control handler that rejects every
//! command, and the address config.

use dnp3::app::control::{
    CommandStatus, Group12Var1, Group41Var1, Group41Var2, Group41Var3, Group41Var4,
};
use dnp3::app::{Listener, MaybeAsync};
use dnp3::link::EndpointAddress;
use dnp3::outstation::database::{DatabaseHandle, EventBufferConfig};
use dnp3::outstation::{
    ConnectionState, ControlHandler, ControlSupport, OperateType, OutstationApplication,
    OutstationConfig, OutstationInformation,
};

/// Minimal OutstationApplication — defaults are fine for read-only use.
pub struct App;
impl OutstationApplication for App {}

/// No-op OutstationInformation.
pub struct Info;
impl OutstationInformation for Info {}

/// No-op ControlHandler. We don't accept any commands in Tier 1; every
/// select/operate returns `NotSupported`.
pub struct Ctl;
impl ControlHandler for Ctl {}

/// Stamp out a `ControlSupport<$ty>` impl that rejects every select/operate
/// with `CommandStatus::NotSupported`. Used to satisfy ControlHandler's trait
/// bounds without writing real command handlers (Tier 1 is read-only).
macro_rules! reject_control {
    ($ty:ty) => {
        impl ControlSupport<$ty> for Ctl {
            fn select(
                &mut self,
                _control: $ty,
                _index: u16,
                _db: &mut DatabaseHandle,
            ) -> CommandStatus {
                CommandStatus::NotSupported
            }
            fn operate(
                &mut self,
                _control: $ty,
                _index: u16,
                _op_type: OperateType,
                _db: &mut DatabaseHandle,
            ) -> CommandStatus {
                CommandStatus::NotSupported
            }
        }
    };
}

reject_control!(Group12Var1);
reject_control!(Group41Var1);
reject_control!(Group41Var2);
reject_control!(Group41Var3);
reject_control!(Group41Var4);

/// No-op connection-state listener.
pub struct NopListener;
impl Listener<ConnectionState> for NopListener {
    fn update(&mut self, _state: ConnectionState) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
}

/// Outstation config — single master at addr 1, this outstation at 1024.
pub fn outstation_config() -> OutstationConfig {
    OutstationConfig::new(
        EndpointAddress::try_new(1024).expect("outstation addr"),
        EndpointAddress::try_new(1).expect("master addr"),
        // Small event buffers; static reads only.
        EventBufferConfig::new(0, 0, 0, 0, 0, 5, 0, 0),
    )
}
