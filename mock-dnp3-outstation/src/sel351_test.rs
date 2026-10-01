//! The SEL-351 profile over a real DNP3 read: analogs in the outstation's
//! float variation, targets as binary inputs, at the indices from SEL's
//! device profile (dnpDP-351R100.xml).

use crate::sel351;
use crate::{App, Ctl, Info, NopListener, outstation_config};
use dnp3::app::measurement::{AnalogInput, BinaryInput};
use dnp3::app::{ConnectStrategy, MaybeAsync, NullListener, ResponseHeader, Variation};
use dnp3::link::{EndpointAddress, LinkErrorMode};
use dnp3::master::{
    AssociationConfig, AssociationHandler, AssociationInformation, Classes, EventClasses,
    HeaderInfo, ReadHandler, ReadRequest, ReadType,
};
use dnp3::tcp::{AddressFilter, EndpointList, Server, spawn_master_tcp_client};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Fixed high port for the in-process outstation under test.
const TEST_PORT: u16 = 35914;

/// Captured values: analogs as-is, binaries as 1.0 / 0.0, keyed by (group, index).
type Captured = Arc<Mutex<HashMap<(u8, u16), f64>>>;

struct Capturing(Captured);
impl ReadHandler for Capturing {
    fn begin_fragment(&mut self, _r: ReadType, _h: ResponseHeader) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
    fn end_fragment(&mut self, _r: ReadType, _h: ResponseHeader) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
    fn handle_analog_input(
        &mut self,
        _i: HeaderInfo,
        iter: &mut dyn Iterator<Item = (AnalogInput, u16)>,
    ) {
        let mut map = self.0.lock().expect("capture lock");
        map.extend(iter.map(|(v, idx)| ((30, idx), v.value)));
    }
    fn handle_binary_input(
        &mut self,
        _i: HeaderInfo,
        iter: &mut dyn Iterator<Item = (BinaryInput, u16)>,
    ) {
        let mut map = self.0.lock().expect("capture lock");
        map.extend(iter.map(|(v, idx)| ((1, idx), if v.value { 1.0 } else { 0.0 })));
    }
}
struct AssocH;
impl AssociationHandler for AssocH {}
struct AssocI;
impl AssociationInformation for AssocI {}

#[tokio::test]
async fn every_sel351_point_reads_back_over_dnp3() {
    // Arrange — in-process outstation seeded with the SEL-351 profile
    let mut server = Server::new_tcp_server(
        LinkErrorMode::Close,
        format!("127.0.0.1:{TEST_PORT}").parse().expect("addr"),
    );
    let outstation = server
        .add_outstation(
            outstation_config(),
            Box::new(App),
            Box::new(Info),
            Box::new(Ctl),
            Box::new(NopListener),
            AddressFilter::Any,
        )
        .expect("add outstation");
    outstation.transaction(sel351::seed);
    let _server_handle = server.bind().await.expect("bind");

    // Act — a master reads every point in the outstation's default variation
    let mut channel = spawn_master_tcp_client(
        LinkErrorMode::Close,
        dnp3::master::MasterChannelConfig::new(EndpointAddress::try_new(1).expect("addr")),
        EndpointList::single(format!("127.0.0.1:{TEST_PORT}")),
        ConnectStrategy::default(),
        NullListener::create(),
    );
    let captured: Captured = Arc::new(Mutex::new(HashMap::new()));
    let mut association = channel
        .add_association(
            EndpointAddress::try_new(1024).expect("addr"),
            AssociationConfig::new(
                EventClasses::none(),
                EventClasses::none(),
                Classes::all(),
                EventClasses::none(),
            ),
            Box::new(Capturing(captured.clone())),
            Box::new(AssocH),
            Box::new(AssocI),
        )
        .await
        .expect("association");
    channel.enable().await.expect("enable");
    association
        .read(ReadRequest::one_byte_range(Variation::Group30Var0, 0, 12))
        .await
        .expect("analog read");
    association
        .read(ReadRequest::one_byte_range(Variation::Group1Var0, 0, 25))
        .await
        .expect("binary read");

    // Assert — the power-engineer's map: currents A, voltages kV, targets 0/1
    let got = captured.lock().expect("capture lock");
    let near = |k: (u8, u16), want: f64| (got[&k] - want).abs() < 1e-3;
    assert!(near((30, 0), 150.0) && near((30, 2), 152.0) && near((30, 4), 148.0));
    assert!(near((30, 8), 7.20) && near((30, 10), 7.18) && near((30, 12), 7.22));
    assert_eq!(got[&(1, 0)], 1.0, "breaker_closed");
    assert_eq!(got[&(1, 9)], 0.0, "trip_status");
    assert_eq!(got[&(1, 15)], 0.0, "ground_fault");
    assert_eq!(got[&(1, 24)], 1.0, "anti_islanding_armed");
    assert_eq!(got[&(1, 25)], 1.0, "ride_through_enabled");
}
