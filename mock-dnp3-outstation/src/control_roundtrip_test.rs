//! Round-trip proof: HTTP control write -> outstation database -> real
//! DNP3 master read. Uses an unseeded point index so seed-on-demand is
//! exercised, and an in-process master (gateway pattern) for the read.

use crate::control::{ControlState, control_router};
use crate::{App, Ctl, Info, NopListener};
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use dnp3::app::measurement::AnalogInput;
use dnp3::app::{ConnectStrategy, MaybeAsync, NullListener, ResponseHeader, Variation};
use dnp3::link::{EndpointAddress, LinkErrorMode};
use dnp3::master::{
    AssociationConfig, AssociationHandler, AssociationInformation, Classes, EventClasses,
    HeaderInfo, ReadHandler, ReadRequest, ReadType,
};
use dnp3::outstation::OutstationConfig;
use dnp3::outstation::database::EventBufferConfig;
use dnp3::tcp::{AddressFilter, EndpointList, Server, spawn_master_tcp_client};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tower::util::ServiceExt;

/// Fixed high port for the in-process outstation under test.
const TEST_PORT: u16 = 35913;

/// ReadHandler capturing AnalogInput values into a shared map.
struct Capturing(Arc<Mutex<HashMap<u16, f64>>>);
impl ReadHandler for Capturing {
    fn begin_fragment(&mut self, _r: ReadType, _h: ResponseHeader) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
    fn end_fragment(&mut self, _r: ReadType, _h: ResponseHeader) -> MaybeAsync<()> {
        MaybeAsync::ready(())
    }
    fn handle_analog_input(
        &mut self,
        _info: HeaderInfo,
        iter: &mut dyn Iterator<Item = (AnalogInput, u16)>,
    ) {
        let mut map = self.0.lock().expect("capture lock poisoned");
        for (ai, idx) in iter {
            map.insert(idx, ai.value);
        }
    }
}
/// No-op association handler.
struct AssocH;
impl AssociationHandler for AssocH {}
/// No-op association information.
struct AssocI;
impl AssociationInformation for AssocI {}

#[tokio::test]
async fn control_write_is_readable_over_real_dnp3() {
    // Arrange — in-process outstation with NO pre-seeded points
    let mut server = Server::new_tcp_server(
        LinkErrorMode::Close,
        format!("127.0.0.1:{TEST_PORT}").parse().expect("addr"),
    );
    let outstation = server
        .add_outstation(
            OutstationConfig::new(
                EndpointAddress::try_new(1024).expect("outstation addr"),
                EndpointAddress::try_new(1).expect("master addr"),
                EventBufferConfig::new(0, 0, 0, 0, 0, 5, 0, 0),
            ),
            Box::new(App),
            Box::new(Info),
            Box::new(Ctl),
            Box::new(NopListener),
            AddressFilter::Any,
        )
        .expect("add outstation");
    let _server_handle = server.bind().await.expect("bind");
    let driven = Arc::new(Mutex::new(HashSet::new()));
    let state = ControlState {
        outstation,
        driven: driven.clone(),
    };

    // Act — control write to an unseeded index (seed-on-demand path)
    let request = Request::builder()
        .method("PUT")
        .uri("/points")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"analog_inputs":{"1":5000000.0}}"#))
        .expect("request");
    let response = control_router(state).oneshot(request).await.expect("route");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Act — real DNP3 master read of point 1
    let mut channel = spawn_master_tcp_client(
        LinkErrorMode::Close,
        dnp3::master::MasterChannelConfig::new(EndpointAddress::try_new(1).expect("addr")),
        EndpointList::single(format!("127.0.0.1:{TEST_PORT}")),
        ConnectStrategy::default(),
        NullListener::create(),
    );
    let captured: Arc<Mutex<HashMap<u16, f64>>> = Arc::new(Mutex::new(HashMap::new()));
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
        .read(ReadRequest::one_byte_range(Variation::Group30Var1, 1, 1))
        .await
        .expect("read");

    // Assert — the driven value came back over the wire, index marked driven
    let value = *captured
        .lock()
        .expect("capture lock")
        .get(&1)
        .expect("point 1 present");
    assert_eq!(value, 5_000_000.0);
    assert!(driven.lock().expect("driven lock").contains(&1));
}
