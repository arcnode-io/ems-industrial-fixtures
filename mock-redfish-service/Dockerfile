# syntax=docker/dockerfile:1.6
FROM rust:1.93-slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY mock-redfish-service/Cargo.toml mock-redfish-service/Cargo.toml
COPY mock-redfish-service/src mock-redfish-service/src
RUN mkdir -p src && echo "" > src/lib.rs
RUN mkdir -p mock-modbus-server/src mock-snmp-agent/src mock-dnp3-outstation/src mock-bacnet-device/src
RUN for m in mock-modbus-server mock-snmp-agent mock-dnp3-outstation mock-bacnet-device; do \
      printf '[package]\nname = "%s"\nversion.workspace = true\nedition.workspace = true\nrust-version.workspace = true\n\n[[bin]]\nname = "%s"\npath = "src/main.rs"\n' "$m" "$m" > $m/Cargo.toml; \
      printf 'fn main() {}\n' > $m/src/main.rs; \
    done
RUN cargo build --release -p mock-redfish-service

FROM debian:bookworm-slim
COPY --from=builder /build/target/release/mock-redfish-service /usr/local/bin/mock-redfish-service
EXPOSE 8443
ENTRYPOINT ["/usr/local/bin/mock-redfish-service"]
