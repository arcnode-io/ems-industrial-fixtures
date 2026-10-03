//! Site load with compute: the non-compute load plus every GPU node's draw,
//! so a GPU cap shows up at the POI.

use super::{ComputeLoad, site_load};

/// The demo site: 98 nodes and 91 kW of everything else.
const DEMO: ComputeLoad = ComputeLoad { nodes: 98.0 };

#[test]
fn full_power_nodes_make_up_the_site_load() {
    // 91 kW + 98 × 10.5 kW = 1.12 MW
    assert_eq!(site_load(91_000.0, Some((&DEMO, 10_500.0))), 1_120_000.0);
}

#[test]
fn capping_the_gpus_lowers_the_site_load() {
    // every GPU capped to 200 W: node = 8 × 200 + 2,500 = 4.1 kW
    assert_eq!(site_load(91_000.0, Some((&DEMO, 4_100.0))), 492_800.0);
}

#[test]
fn without_compute_the_site_load_is_the_fixed_load() {
    assert_eq!(site_load(1_120_000.0, None), 1_120_000.0);
}
