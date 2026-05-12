# syntax=docker/dockerfile:1.6
FROM rust:1.93-slim AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY mock-modbus-server/Cargo.toml mock-modbus-server/Cargo.toml
COPY mock-modbus-server/src mock-modbus-server/src
# The workspace Cargo.toml has a [package] section with no targets — give it a
# stub lib.rs so cargo accepts it without building anything substantive.
RUN mkdir -p src && echo "" > src/lib.rs
# Stub the other workspace members so cargo's resolver doesn't need their trees.
RUN mkdir -p mock-snmp-agent/src mock-redfish-service/src mock-dnp3-outstation/src mock-bacnet-device/src
RUN for m in mock-snmp-agent mock-redfish-service mock-dnp3-outstation mock-bacnet-device; do \
      printf '[package]\nname = "%s"\nversion.workspace = true\nedition.workspace = true\nrust-version.workspace = true\n\n[[bin]]\nname = "%s"\npath = "src/main.rs"\n' "$m" "$m" > $m/Cargo.toml; \
      printf 'fn main() {}\n' > $m/src/main.rs; \
    done
RUN cargo build --release -p mock-modbus-server

FROM debian:bookworm-slim
COPY --from=builder /build/target/release/mock-modbus-server /usr/local/bin/mock-modbus-server
EXPOSE 502
ENTRYPOINT ["/usr/local/bin/mock-modbus-server"]
