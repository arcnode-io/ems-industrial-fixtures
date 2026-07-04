# EMS Industrial Fixtures 🎭⚙️

![](https://img.shields.io/gitlab/pipeline-status/arcnode-io/ems-industrial-fixtures?branch=main&logo=gitlab)
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
  rectangle mock_redfish_service
  rectangle mock_dnp3_outstation
  rectangle mock_canbus_node
}

industrial_gateway --> mock_modbus_server: Modbus TCP
industrial_gateway --> mock_snmp_agent: SNMP
industrial_gateway --> mock_redfish_service: Redfish HTTPS
industrial_gateway --> mock_dnp3_outstation: DNP3
industrial_gateway --> mock_canbus_node: CANbus
```

## Fixtures

| Protocol | Port | Description |
|----------|------|-------------|
| Modbus TCP | 502 | Mock modbus server |
| SNMP | 161 | Mock snmp agent |
| Redfish | 8443 | Mock redfish service |
| DNP3 | 20000 | Mock dnp3 outstation |
| CANbus | - | Mock canbus node |

### Control surface (digital-twin)

`mock-modbus-server` exposes an out-of-band HTTP control endpoint
(`CONTROL_PORT`, default 8080) so the `digital-twin` service can drive
register values with coherent plant physics instead of the built-in
sawtooth:

```bash
curl -X PUT localhost:8080/registers \
  -H 'content-type: application/json' \
  -d '{"registers": {"4000": 15, "4001": 16960}}'
```

Batch writes apply atomically under one lock (int32 word pairs can't tear
mid-poll). Driven addresses are excluded from the sawtooth simulator from
then on; untouched channels keep drifting. The Modbus surface itself stays
read-only (TLS mode's authz denies protocol writes by design). Sim fixture
only — never expose the control port beyond the deployment network.

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
├── mock-redfish-service/   # Redfish HTTPS/JSON service mock
├── mock-dnp3-outstation/   # DNP3 outstation mock
└── mock-canbus-node/       # CANbus node mock
```

