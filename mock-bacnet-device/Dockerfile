# syntax=docker/dockerfile:1.6
FROM rust:1.93-slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY mock-bacnet-device/Cargo.toml mock-bacnet-device/Cargo.toml
COPY mock-bacnet-device/src mock-bacnet-device/src
RUN mkdir -p src && echo "" > src/lib.rs
RUN mkdir -p mock-modbus-server/src mock-snmp-agent/src mock-redfish-service/src mock-dnp3-outstation/src
RUN for m in mock-modbus-server mock-snmp-agent mock-redfish-service mock-dnp3-outstation; do \
      printf '[package]\nname = "%s"\nversion.workspace = true\nedition.workspace = true\nrust-version.workspace = true\n\n[[bin]]\nname = "%s"\npath = "src/main.rs"\n' "$m" "$m" > $m/Cargo.toml; \
      printf 'fn main() {}\n' > $m/src/main.rs; \
    done
RUN cargo build --release -p mock-bacnet-device

FROM debian:bookworm-slim
COPY --from=builder /build/target/release/mock-bacnet-device /usr/local/bin/mock-bacnet-device
EXPOSE 47808/udp
ENTRYPOINT ["/usr/local/bin/mock-bacnet-device"]
