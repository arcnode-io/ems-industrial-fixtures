# syntax=docker/dockerfile:1.6
FROM rust:1.93-slim AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY mock-snmp-agent/Cargo.toml mock-snmp-agent/Cargo.toml
COPY mock-snmp-agent/src mock-snmp-agent/src
RUN mkdir -p src && echo "" > src/lib.rs
RUN mkdir -p mock-modbus-server/src mock-redfish-service/src mock-dnp3-outstation/src mock-bacnet-device/src
RUN for m in mock-modbus-server mock-redfish-service mock-dnp3-outstation mock-bacnet-device; do \
      printf '[package]\nname = "%s"\nversion.workspace = true\nedition.workspace = true\nrust-version.workspace = true\n\n[[bin]]\nname = "%s"\npath = "src/main.rs"\n' "$m" "$m" > $m/Cargo.toml; \
      printf 'fn main() {}\n' > $m/src/main.rs; \
    done
RUN cargo build --release -p mock-snmp-agent

FROM debian:bookworm-slim
COPY --from=builder /build/target/release/mock-snmp-agent /usr/local/bin/mock-snmp-agent
EXPOSE 161/udp
ENTRYPOINT ["/usr/local/bin/mock-snmp-agent"]
