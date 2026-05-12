//! mock-dnp3-outstation — DNP3 TCP server fixture for gateway testing.
//! One analog input point (index 0) driven by a simulator.

mod simulator;

use dnp3::app::control::{
    CommandStatus, Group12Var1, Group41Var1, Group41Var2, Group41Var3, Group41Var4,
};
use dnp3::app::{Listener, MaybeAsync};
use dnp3::link::{EndpointAddress, LinkErrorMode};
use dnp3::outstation::database::{
    Add, AnalogInputConfig, DatabaseHandle, EventAnalogInputVariation, EventBufferConfig,
    EventClass, StaticAnalogInputVariation,
};
use dnp3::outstation::{
    ConnectionState, ControlHandler, ControlSupport, OperateType, OutstationApplication,
    OutstationConfig, OutstationInformation,
};
use dnp3::tcp::{AddressFilter, Server};
use simulator::Simulator;
use std::time::Duration;
use tracing::info;

/// Minimal OutstationApplication — defaults are fine for read-only use.
struct App;
impl OutstationApplication for App {}

/// No-op OutstationInformation.
struct Info;
impl OutstationInformation for Info {}

/// No-op ControlHandler. We don't accept any commands in Tier 1; every
/// select/operate returns `NotSupported`.
struct Ctl;
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
struct NopListener;
impl Listener<ConnectionState> for NopListener {
    fn update(&mut self, _state: ConnectionState) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_target(false).init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20000);
    let tick_ms: u64 = std::env::var("TICK_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);

    let mut server =
        Server::new_tcp_server(LinkErrorMode::Close, format!("0.0.0.0:{port}").parse()?);
    let outstation = server.add_outstation(
        outstation_config(),
        Box::new(App),
        Box::new(Info),
        Box::new(Ctl),
        Box::new(NopListener),
        AddressFilter::Any,
    )?;

    // Seed the database with one analog input point at index 0.
    outstation.transaction(|db| {
        db.add(
            0,
            Some(EventClass::Class1),
            AnalogInputConfig {
                s_var: StaticAnalogInputVariation::Group30Var1,
                e_var: EventAnalogInputVariation::Group32Var1,
                deadband: 0.0,
            },
        );
    });

    let _server_handle = server.bind().await?;
    info!(%port, tick_ms, "mock-dnp3-outstation listening");

    // Simulator tick.
    let mut sim = Simulator::new();
    let outstation_for_sim = outstation.clone();
    tokio::spawn(async move {
        loop {
            sim.tick(&outstation_for_sim);
            tokio::time::sleep(Duration::from_millis(tick_ms)).await;
        }
    });

    tokio::signal::ctrl_c().await?;
    Ok(())
}

/// Outstation config — single master at addr 1, this outstation at 1024.
fn outstation_config() -> OutstationConfig {
    OutstationConfig::new(
        EndpointAddress::try_new(1024).expect("outstation addr"),
        EndpointAddress::try_new(1).expect("master addr"),
        // Small event buffers; we only have one analog input.
        EventBufferConfig::new(0, 0, 0, 0, 0, 5, 0, 0),
    )
}
