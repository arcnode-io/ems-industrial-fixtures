//! POI meter instantaneous active power (poi_meter profile, float32 at 3060).
//!
//! `P_poi = SITE_LOAD_W − Σ rack active_power` (+ = import from grid), per
//! Joe's SME review §6b. The racks are separate containers, so this reads
//! each one's active_power over Modbus, the same way a real meter sees the
//! battery's output at the connection point.
//!
//! Env: `SITE_LOAD_W` (default 0; set it to the Redfish GPU total plus
//! auxiliary load) and `POI_BESS_RACKS` (comma-separated `host:port`, default
//! none, in which case P_poi is just the site load).

use crate::handler::MeterHandler;
use rodbus::client::{Channel, HostAddr, RequestParam, spawn_tcp_client_task};
use rodbus::{AddressRange, DecodeLevel, UnitId};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::warn;

/// poi_meter active_power register pair (int32 high_low), per edp-api 9e99d37.
const POI_ACTIVE_POWER: u16 = 3060;
/// bess_rack active_power register pair (int32 high_low).
const RACK_ACTIVE_POWER: u16 = 10;
/// Per-read timeout against a rack.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// P_poi, + = import from grid.
pub fn poi_active_power(site_load_w: f64, rack_powers_w: &[f64]) -> f64 {
    site_load_w - rack_powers_w.iter().sum::<f64>()
}

/// Write P_poi to 3060-3061 (float32 high_low) unless either word is driven.
pub fn write_poi_power(holding: &mut HashMap<u16, u16>, driven: &HashSet<u16>, p_w: f64) {
    if driven.contains(&POI_ACTIVE_POWER) || driven.contains(&(POI_ACTIVE_POWER + 1)) {
        return;
    }
    #[allow(clippy::cast_possible_truncation)]
    let raw = (p_w as f32).to_bits();
    holding.insert(POI_ACTIVE_POWER, (raw >> 16) as u16);
    holding.insert(POI_ACTIVE_POWER + 1, raw as u16);
}

/// Parse `POI_BESS_RACKS` + `SITE_LOAD_W` and, when either is set, spawn the
/// tick task. A failed rack read keeps the last written value rather than
/// publishing a sum that's missing a rack.
pub fn spawn_from_env(
    handler: Arc<Mutex<Box<MeterHandler>>>,
    tick: Duration,
) -> Result<(), String> {
    let site_load = match std::env::var("SITE_LOAD_W") {
        Ok(raw) => raw
            .parse::<f64>()
            .map_err(|_| format!("SITE_LOAD_W is not a number: {raw}"))?,
        Err(_) => 0.0,
    };
    let racks = std::env::var("POI_BESS_RACKS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| parse_host_port(s.trim()))
        .collect::<Result<Vec<_>, _>>()?;
    let channels: Vec<Channel> = racks
        .into_iter()
        .map(|(host, port)| {
            spawn_tcp_client_task(
                HostAddr::dns(host, port),
                1,
                rodbus::default_retry_strategy(),
                DecodeLevel::default(),
                None,
            )
        })
        .collect();
    tokio::spawn(async move {
        for ch in &channels {
            let _ = ch.enable().await;
        }
        loop {
            let mut powers = Vec::with_capacity(channels.len());
            for ch in &channels {
                match read_rack_power(ch).await {
                    Some(p) => powers.push(p),
                    None => break,
                }
            }
            if powers.len() == channels.len() {
                let p_poi = poi_active_power(site_load, &powers);
                let mut guard = handler.lock().unwrap();
                let h = &mut **guard;
                write_poi_power(&mut h.holding, &h.driven, p_poi);
            } else {
                warn!("POI meter: a rack read failed; holding last active_power");
            }
            tokio::time::sleep(tick).await;
        }
    });
    Ok(())
}

/// One rack's active_power (W), or None if the read failed.
async fn read_rack_power(channel: &Channel) -> Option<f64> {
    let range = AddressRange::try_from(RACK_ACTIVE_POWER, 2).ok()?;
    let param = RequestParam::new(UnitId::new(1), READ_TIMEOUT);
    let mut ch = channel.clone();
    let words = ch.read_holding_registers(param, range).await.ok()?;
    let (high, low) = (words.first()?.value, words.get(1)?.value);
    Some(f64::from(((u32::from(high) << 16) | u32::from(low)) as i32))
}

/// Parse `host:port`.
fn parse_host_port(s: &str) -> Result<(String, u16), String> {
    let (host, port) = s
        .rsplit_once(':')
        .ok_or_else(|| format!("POI_BESS_RACKS entry needs host:port, got {s}"))?;
    let port = port
        .parse()
        .map_err(|_| format!("bad port in POI_BESS_RACKS entry {s}"))?;
    Ok((host.to_string(), port))
}

#[cfg(test)]
#[path = "poi_test.rs"]
mod tests;
