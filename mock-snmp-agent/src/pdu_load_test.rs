//! A load-following PDU: its input power is its share of what it feeds, and
//! its phase currents carry that power at the phase voltages it reports.

use super::{PduLoad, readings};
use crate::oids::initial_values;

/// One of a compute container's four 2N PDUs: 7 GPU nodes + 3 kW of CDU and
/// switch.
const CONTAINER_PDU: PduLoad = PduLoad {
    nodes: 7.0,
    share: 4.0,
    base_w: 3_000.0,
};

fn oid(table: u32, tail: &[u32]) -> Vec<u32> {
    [
        &[1, 3, 6, 1, 4, 1, 1718, 4, 1, table, 3, 1, 3, 1, 1][..],
        tail,
    ]
    .concat()
}

#[test]
fn input_power_is_this_pdus_share_of_its_loads() {
    // Act — nodes at 10.5 kW each
    let r = readings(&CONTAINER_PDU, 10_500.0, &initial_values());
    // Assert — (7 × 10.5 kW + 3 kW) / 4
    assert_eq!(r[&oid(3, &[])], 19_125);
}

#[test]
fn phase_currents_carry_that_power_at_each_phase_voltage() {
    // Arrange
    let values = initial_values();
    // Act
    let r = readings(&CONTAINER_PDU, 10_500.0, &values);
    // Assert — Σ V × I is the input power, within rounding
    let va: f64 = (1..=3)
        .map(|p| r[&oid(4, &[p])] as f64 * 0.01 * values[&oid(5, &[p])] as f64 * 0.1)
        .sum();
    assert!((va - 19_125.0).abs() < 5.0, "{va}");
}

#[test]
fn capped_gpus_lower_the_pdus_power() {
    // GPUs capped from 1000 to 810 W: node 10.5 → 8.98 kW
    let full = readings(&CONTAINER_PDU, 10_500.0, &initial_values())[&oid(3, &[])];
    let capped = readings(&CONTAINER_PDU, 8_980.0, &initial_values())[&oid(3, &[])];
    assert!(capped < full);
}

#[tokio::test]
async fn store_never_holds_values_while_waiting_on_driven() {
    // Arrange — the drift sim mid-tick holds `driven` and wants `values`
    let values: crate::control::SharedValues = Default::default();
    let driven: crate::control::DrivenSet = Default::default();
    let sim_holds = driven.clone().lock_owned().await;
    let next = readings(&CONTAINER_PDU, 10_500.0, &initial_values());
    // Act
    let pdu = tokio::spawn(super::store(values.clone(), driven.clone(), next));
    tokio::task::yield_now().await;
    // Assert — the sim can still take `values`, so neither side deadlocks
    assert!(values.try_lock().is_ok());
    drop(sim_holds);
    pdu.await.unwrap();
}
