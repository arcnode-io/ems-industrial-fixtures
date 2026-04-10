# EMS Industrial Fixtures 🎭⚙️

![](https://img.shields.io/gitlab/pipeline-status/arcnode-io/ems-industrial-fixtures?branch=main&logo=gitlab)
![](https://gitlab.com/arcnode-io/ems-industrial-fixtures/badges/main/coverage.svg)
![](https://img.shields.io/badge/1.93-gray?logo=rust)

> Mock protocol implementations for industrial gateway testing

## Pre-requisites
- rust 1.93+

## Diagrams

### Context
```plantuml
rectangle industrial_gateway
rectangle fixtures #line.dashed {
  rectangle mock_modbus_server
  rectangle mock_snmp_agent
  rectangle mock_bacnet_server
  rectangle mock_dnp3_outstation
  rectangle mock_ocpp_station
  rectangle mock_canbus_node
}

industrial_gateway --- mock_modbus_server
industrial_gateway -- mock_snmp_agent
industrial_gateway -- mock_bacnet_server
industrial_gateway -- mock_dnp3_outstation
industrial_gateway -- mock_ocpp_station
industrial_gateway -- mock_canbus_node
```

## Fixtures

| Protocol | Port | Description |
|----------|------|-------------|
| Modbus TCP | 502 | Mock modbus server |
| SNMP | 161 | Mock snmp agent |
| BACnet | 47808 | Mock bacnet server |
| DNP3 | 20000 | Mock dnp3 outstation |
| OCPP | 8080 | Mock ocpp station |
| CANbus | - | Mock canbus node |

## Docker

Each fixture runs in its own container for isolated testing:

```bash
# Build all fixture containers
docker-compose build

# Run all fixtures
docker-compose up

# Run specific fixture
docker-compose up mock-modbus-server
```

## Usage

```bash
# Run specific fixture locally
cargo run --bin mock-modbus-server
cargo run --bin mock-snmp-agent

# Test all fixtures
cargo test
```

## Project Structure
```
├── Cargo.toml              # Workspace configuration
├── mock-modbus-server/     # Modbus TCP/RTU mock
├── mock-snmp-agent/        # SNMP agent mock
├── mock-bacnet-server/     # BACnet server mock
├── mock-dnp3-outstation/   # DNP3 outstation mock
├── mock-ocpp-station/      # OCPP charging station mock
└── mock-canbus-node/       # CANbus node mock
```

