//! cdu's Redfish resources, the DMTF CoolingUnit model under
//! `/ThermalEquipment/CDUs/1`, shaped to edp-api's cdu.yaml json_pointers.
//!
//! Supply 30 °C, return 38 °C at 120 L/min moves ~67 kW, about a rack of
//! CMP-NODE-001s. Illustrative; no spec gives these figures.

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

/// Secondary-loop supply temperature.
const SUPPLY_C: f64 = 30.0;
/// Secondary-loop return temperature.
const RETURN_C: f64 = 38.0;
/// Secondary-loop flow.
const FLOW_LPM: f64 = 120.0;
/// Pump duty.
const PUMP_PERCENT: f64 = 65.0;

/// Redfish CoolantConnector for the secondary loop.
pub fn secondary_connector_json() -> Value {
    json!({
        "@odata.id": "/redfish/v1/ThermalEquipment/CDUs/1/SecondaryCoolantConnectors/1",
        "@odata.type": "#CoolantConnector.v1_1_0.CoolantConnector",
        "Id": "1",
        "Name": "Secondary Loop",
        "SupplyTemperatureCelsius": { "Reading": SUPPLY_C },
        "ReturnTemperatureCelsius": { "Reading": RETURN_C },
        "FlowLitersPerMinute": { "Reading": FLOW_LPM },
    })
}

/// Redfish Pump; `Status/State` is the text pump_state's value_map decodes.
pub fn pump_json() -> Value {
    json!({
        "@odata.id": "/redfish/v1/ThermalEquipment/CDUs/1/Pumps/1",
        "@odata.type": "#Pump.v1_1_0.Pump",
        "Id": "1",
        "Name": "Pump 1",
        "PumpSpeedPercent": { "Reading": PUMP_PERCENT },
        "Status": { "State": "Enabled", "Health": "OK" },
    })
}

/// Routes for the CDU's resources.
pub fn router() -> Router {
    Router::new()
        .route(
            "/redfish/v1/ThermalEquipment/CDUs/1/SecondaryCoolantConnectors/1",
            get(|| async { Json(secondary_connector_json()) }),
        )
        .route(
            "/redfish/v1/ThermalEquipment/CDUs/1/Pumps/1",
            get(|| async { Json(pump_json()) }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_secondary_loop_serves_supply_return_and_flow() {
        let json = secondary_connector_json();
        assert_eq!(
            json.pointer("/SupplyTemperatureCelsius/Reading"),
            Some(&30.0.into())
        );
        assert_eq!(
            json.pointer("/ReturnTemperatureCelsius/Reading"),
            Some(&38.0.into())
        );
        assert_eq!(
            json.pointer("/FlowLitersPerMinute/Reading"),
            Some(&120.0.into())
        );
    }

    #[test]
    fn the_pump_serves_speed_and_a_dmtf_state() {
        let json = pump_json();
        assert_eq!(
            json.pointer("/PumpSpeedPercent/Reading"),
            Some(&65.0.into())
        );
        assert_eq!(json.pointer("/Status/State"), Some(&"Enabled".into()));
    }
}
