# Mock Modbus Server 📟

![](https://img.shields.io/gitlab/pipeline-status/arcnode-io/ems-industrial-fixtures?branch=main&logo=gitlab)
![](https://img.shields.io/badge/1.93-gray?logo=rust)

## Pre-Requisites

 ```shell
cargo install cargo-cmd \
               cargo-audit \
               cargo-udeps --locked \
               cargo2junit
```

## bess_rack profile

`MODBUS_PROFILE=bess_rack` simulates one 1927 kW / 3854 kWh rack (edp-api
`bess_rack.yaml`). Env: `BATTERY_INITIAL_SOC_PERCENT` (default 60) and
`BATTERY_TIME_SCALE` (default 1; simulated seconds per wall second).

`max_charge_power` (60-61) and `max_discharge_power` (62-63) follow a
**fixture-chosen** SoC derate, not the real Megapack's taper, which isn't
published:

- discharge: rated at ≥ 20% SoC, linear to 0 W at 0%
- charge: rated at ≤ 90% SoC, linear to 0 W at 100%

The rack's output obeys the same limits. With the demo's 30.6% supplier
reserve floor, discharge stops before its taper starts, so only the charge
side shows unless the floor is lowered.
