# Raspberry Pi Deployment Guide

## Overview

This guide deploys RuView in full-feature Raspberry Pi mode:

1. Pi node agent ingests Nexmon CSI.
2. Node agent emits canonical packets (`0xC5110001..0xC5110006`).
3. Sensing server runs with `--source nexmon`.

## Prerequisites

- Raspberry Pi 4 with supported Nexmon firmware.
- UDP connectivity between node agent and sensing server.
- Rust toolchain (for source builds) or prebuilt binaries.
- `systemd` (for managed services).

## Build

```bash
cd rust-port/wifi-densepose-rs
cargo build -p wifi-densepose-pi-node-agent -p wifi-densepose-sensing-server --release
```

## Run (manual)

Terminal 1:

```bash
./target/release/sensing-server --source nexmon --nexmon-port 5500 --udp-port 5005 --bind-addr 0.0.0.0
```

Terminal 2:

```bash
./target/release/wifi-densepose-pi-node-agent --listen 0.0.0.0:5500 --aggregator 127.0.0.1:5005 --tier 2 --enable-wasm --mmwave-mock
```

## Install as services

```bash
bash deploy/pi/install.sh
```

Validate unit files:

```bash
systemd-analyze verify deploy/pi/systemd/ruview-pi-agent.service deploy/pi/systemd/ruview-sensing-server.service
```

## Verification Matrix

1. Protocol tests:

```bash
cargo test -p wifi-densepose-sensing-server protocol_packet_test edge_packet_test nexmon_ingest_test -- --nocapture
```

2. Pi agent tests:

```bash
cargo test -p wifi-densepose-pi-node-agent -- --nocapture
```

3. Packet loss and latency gate:

- Loss < 2% at 20 Hz
- p95 latency < 120 ms

## Migration Notes (ESP32 -> Pi)

- `0xC5110006` is the canonical WASM event packet magic.
- Python bridge is compatibility-only; native Pi node agent is preferred.
- Existing endpoints remain stable:
  - `/health`
  - `/api/v1/sensing`
  - `/api/v1/edge-vitals`
  - `/api/v1/wasm-events`
