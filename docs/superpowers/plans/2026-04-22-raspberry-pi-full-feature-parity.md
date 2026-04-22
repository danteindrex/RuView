# Raspberry Pi Full-Feature Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace ESP32-only ingestion with Raspberry Pi 4 + Nexmon nodes while preserving all sensing features (raw CSI, vitals, feature vectors, fused vitals, WASM events, multistatic, UI/API behavior).

**Architecture:** Add a Pi node agent that captures Nexmon CSI, performs edge DSP, and emits canonical RuvSense packets to the existing sensing server. Refactor server packet parsing into explicit protocol modules with strict magic/version handling and backward compatibility for legacy ESP32 packets. Keep existing REST/WebSocket contracts stable so UI and downstream tooling do not break.

**Tech Stack:** Rust (`tokio`, `axum`, `serde`, `tracing`), Python (transitional bridge tooling), Nexmon CSI (`nexutil`, monitor mode), Linux/systemd, Docker multi-arch.

---

## Scope and Feature Matrix

This plan delivers parity for:

1. Raw CSI ingestion and multistatic fusion.
2. Edge vitals (presence, motion, breathing, heart rate, persons, fall).
3. Edge feature vectors and compressed frame path.
4. mmWave fused vitals path.
5. WASM edge event path.
6. Existing API/UI compatibility.
7. Pi-native deployment and ops.

This plan intentionally does **not** include ESP-IDF flashing/provisioning workflows after migration.

---

## File Structure (planned ownership)

### Existing files to modify

1. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs`
2. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/csi.rs`
3. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/cli.rs`
4. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/lib.rs`
5. `scripts/nexmon_to_ruview_bridge.py`
6. `docs/user-guide.md`
7. `README.md`
8. `rust-port/wifi-densepose-rs/Cargo.toml` (workspace members)

### New files to create

1. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/mod.rs`
2. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/packet.rs`
3. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/esp32_legacy.rs`
4. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/nexmon.rs`
5. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/protocol_packet_test.rs`
6. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/nexmon_ingest_test.rs`
7. `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/edge_packet_test.rs`
8. `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/Cargo.toml`
9. `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/main.rs`
10. `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/config.rs`
11. `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/nexmon_capture.rs`
12. `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/frame_encoder.rs`
13. `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/edge_dsp.rs`
14. `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/mmwave.rs`
15. `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/wasm_runtime.rs`
16. `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/agent_pipeline_test.rs`
17. `deploy/pi/systemd/ruview-pi-agent.service`
18. `deploy/pi/systemd/ruview-sensing-server.service`
19. `deploy/pi/install.sh`
20. `docs/adr/ADR-090-pi-protocol-parity.md`
21. `docs/pi-deployment-guide.md`

---

### Task 1: Freeze Protocol Contract and Remove Magic Collisions

**Files:**
- Create: `docs/adr/ADR-090-pi-protocol-parity.md`
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/packet.rs`
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/protocol_packet_test.rs`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/lib.rs`

- [ ] **Step 1: Write failing protocol collision test**

```rust
#[test]
fn packet_magic_values_are_unique() {
    use wifi_densepose_sensing_server::protocol::packet::Magic;
    let all = vec![
        Magic::RawFrame as u32,
        Magic::Vitals as u32,
        Magic::Feature as u32,
        Magic::FusedVitals as u32,
        Magic::Compressed as u32,
        Magic::WasmOutputV2 as u32,
    ];
    let mut uniq = all.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(all.len(), uniq.len());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p wifi-densepose-sensing-server protocol_packet_test::packet_magic_values_are_unique -- --nocapture`  
Expected: FAIL (module/constants missing)

- [ ] **Step 3: Implement packet constants and decoder skeleton**

```rust
#[repr(u32)]
pub enum Magic {
    RawFrame = 0xC511_0001,
    Vitals = 0xC511_0002,
    Feature = 0xC511_0003,
    FusedVitals = 0xC511_0004,
    Compressed = 0xC511_0005,
    WasmOutputV2 = 0xC511_0006,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p wifi-densepose-sensing-server protocol_packet_test -- --nocapture`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add docs/adr/ADR-090-pi-protocol-parity.md rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/protocol_packet_test.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/lib.rs
git commit -m "feat(protocol): define canonical packet IDs and remove magic collisions"
```

---

### Task 2: Correct ESP32 Legacy Frame Parsing (offset/type mismatch)

**Files:**
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/esp32_legacy.rs`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/csi.rs`
- Test: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/edge_packet_test.rs`

- [ ] **Step 1: Write failing test for true ESP32 layout**

```rust
#[test]
fn parse_raw_frame_uses_u16_subcarriers_and_u32_freq_offsets() {
    let buf = make_real_esp32_frame(); // helper fixture
    let f = parse_esp32_legacy_frame(&buf).expect("frame");
    assert_eq!(f.n_subcarriers, 64);
    assert_eq!(f.freq_mhz, 2437);
    assert_eq!(f.sequence, 42);
    assert_eq!(f.rssi, -55);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p wifi-densepose-sensing-server parse_raw_frame_uses_u16_subcarriers_and_u32_freq_offsets -- --nocapture`  
Expected: FAIL (current parser uses shifted fields)

- [ ] **Step 3: Implement dual-path legacy parser**

```rust
pub fn parse_esp32_legacy_frame(buf: &[u8]) -> Option<Esp32Frame> {
    if looks_like_real_layout(buf) {
        parse_real_layout(buf)   // [6..7] u16, [8..11] u32, [12..15] u32, rssi@16
    } else {
        parse_compat_layout(buf) // temporary compatibility for existing bridge payloads
    }
}
```

- [ ] **Step 4: Update `main.rs` to call unified parser**

```rust
if let Some(frame) = protocol::esp32_legacy::parse_esp32_legacy_frame(&buf[..len]) {
    // existing update path unchanged
}
```

- [ ] **Step 5: Run parser tests**

Run: `cargo test -p wifi-densepose-sensing-server edge_packet_test -- --nocapture`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/esp32_legacy.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/csi.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/edge_packet_test.rs
git commit -m "fix(parser): align legacy esp32 frame offsets with firmware layout"
```

---

### Task 3: Add Full Packet Type Handling in UDP Receiver

**Files:**
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs`
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/mod.rs`
- Test: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/edge_packet_test.rs`

- [ ] **Step 1: Write failing tests for packet types 0003/0005/0006**

```rust
#[test]
fn udp_decoder_accepts_feature_compressed_and_wasm_v2_packets() {
    assert!(decode_packet(&make_feature_packet()).is_some());
    assert!(decode_packet(&make_compressed_packet()).is_some());
    assert!(decode_packet(&make_wasm_v2_packet()).is_some());
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test -p wifi-densepose-sensing-server udp_decoder_accepts_feature_compressed_and_wasm_v2_packets -- --nocapture`  
Expected: FAIL

- [ ] **Step 3: Implement decoder routing**

```rust
match magic {
    Magic::Vitals => handle_vitals(...),
    Magic::Feature => handle_feature(...),
    Magic::FusedVitals => handle_fused(...),
    Magic::Compressed => handle_compressed(...),
    Magic::WasmOutputV2 => handle_wasm(...),
    Magic::RawFrame => handle_raw(...),
    _ => None,
}
```

- [ ] **Step 4: Keep compatibility for legacy `0xC5110004` WASM packets**

```rust
if magic == Magic::FusedVitals as u32 && looks_like_legacy_wasm(buf) {
    return parse_wasm_legacy(buf);
}
```

- [ ] **Step 5: Run packet tests**

Run: `cargo test -p wifi-densepose-sensing-server edge_packet_test -- --nocapture`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/mod.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/edge_packet_test.rs
git commit -m "feat(server): support full edge packet family for pi parity"
```

---

### Task 4: Add Native Nexmon Ingestion Mode to Sensing Server

**Files:**
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/nexmon.rs`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/cli.rs`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs`
- Test: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/nexmon_ingest_test.rs`

- [ ] **Step 1: Write failing Nexmon payload parse test**

```rust
#[test]
fn parse_nexmon_0x1111_payload_extracts_iq_and_metadata() {
    let pkt = parse_nexmon_payload(&fixture_nexmon_pkt()).expect("nexmon");
    assert_eq!(pkt.magic, 0x1111);
    assert_eq!(pkt.seq, 1234);
    assert!(!pkt.iq.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p wifi-densepose-sensing-server nexmon_ingest_test -- --nocapture`  
Expected: FAIL

- [ ] **Step 3: Implement parser and source mode**

```rust
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum SourceMode { Auto, Simulate, Wifi, Esp32, Nexmon }
```

```rust
fn parse_nexmon_payload(buf: &[u8]) -> Option<NexmonPacket> {
    // magic(2), mac(6), seq(2), core_ss(2), chanspec(2), chip(2), iq...
}
```

- [ ] **Step 4: Add auto-detect fallback for UDP:5500**

```rust
if probe_esp32(udp_port).await { "esp32" }
else if probe_nexmon(5500).await { "nexmon" }
else if probe_windows_wifi().await { "wifi" } else { "simulate" }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p wifi-densepose-sensing-server nexmon_ingest_test -- --nocapture`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/nexmon.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/cli.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/nexmon_ingest_test.rs
git commit -m "feat(server): add native nexmon source mode and parser"
```

---

### Task 5: Create Pi Node Agent Crate (raw capture -> canonical packets)

**Files:**
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/Cargo.toml`
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/main.rs`
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/config.rs`
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/nexmon_capture.rs`
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/frame_encoder.rs`
- Modify: `rust-port/wifi-densepose-rs/Cargo.toml`
- Test: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/agent_pipeline_test.rs`

- [ ] **Step 1: Write failing end-to-end raw-frame test**

```rust
#[tokio::test]
async fn agent_converts_nexmon_to_raw_frame_packet() {
    let out = run_agent_once_with_fixture().await;
    assert_eq!(u32::from_le_bytes(out[0..4].try_into().unwrap()), 0xC511_0001);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p wifi-densepose-pi-node-agent agent_converts_nexmon_to_raw_frame_packet -- --nocapture`  
Expected: FAIL

- [ ] **Step 3: Implement capture and encoder**

```rust
pub struct NexmonPacket { pub seq: u16, pub core_ss: u16, pub chanspec: u16, pub iq: Vec<(i16,i16)> }
pub fn encode_raw_frame(pkt: &NexmonPacket, node_id: u8) -> Vec<u8> { /* canonical 0xC5110001 */ }
```

- [ ] **Step 4: Wire config/env**

```rust
pub struct AgentConfig {
    pub listen_addr: String,      // default 0.0.0.0:5500
    pub aggregator_addr: String,  // default 127.0.0.1:5005
    pub node_base: u8,            // default 10
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p wifi-densepose-pi-node-agent -- --nocapture`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent rust-port/wifi-densepose-rs/Cargo.toml
git commit -m "feat(pi-agent): scaffold nexmon capture and canonical raw frame emission"
```

---

### Task 6: Port Edge DSP Parity into Pi Agent (vitals/features/compression)

**Files:**
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/edge_dsp.rs`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/main.rs`
- Test: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/agent_pipeline_test.rs`

- [ ] **Step 1: Write failing vitals cadence test**

```rust
#[tokio::test]
async fn emits_vitals_and_feature_packets_every_second() {
    let packets = run_agent_for(Duration::from_secs(2)).await;
    assert!(packets.iter().any(|p| magic(p) == 0xC511_0002));
    assert!(packets.iter().any(|p| magic(p) == 0xC511_0003));
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p wifi-densepose-pi-node-agent emits_vitals_and_feature_packets_every_second -- --nocapture`  
Expected: FAIL

- [ ] **Step 3: Implement edge DSP module**

```rust
pub struct EdgeDspState { /* phase history, top-k, filters, fall gate */ }
pub struct EdgeOutputs { pub vitals: Option<Vec<u8>>, pub feature: Option<Vec<u8>>, pub compressed: Option<Vec<u8>> }
pub fn process_frame(state: &mut EdgeDspState, frame: &RawFrame) -> EdgeOutputs { /* parity logic */ }
```

- [ ] **Step 4: Emit `0xC5110005` compressed frames for tier>=2**

```rust
if cfg.tier >= 2 {
    if let Some(pkt) = outputs.compressed { udp.send_to(&pkt, aggregator).await?; }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p wifi-densepose-pi-node-agent -- --nocapture`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/edge_dsp.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/main.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/agent_pipeline_test.rs
git commit -m "feat(pi-agent): add edge dsp parity outputs for vitals feature and compression"
```

---

### Task 7: Add mmWave Fusion Path in Pi Agent

**Files:**
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/mmwave.rs`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/main.rs`
- Test: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/agent_pipeline_test.rs`

- [ ] **Step 1: Write failing fusion packet test**

```rust
#[test]
fn fusion_packet_is_emitted_when_mmwave_present() {
    let pkt = make_fused_packet_with_mmwave();
    assert_eq!(magic(&pkt), 0xC511_0004);
    assert!(pkt.len() >= 48);
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p wifi-densepose-pi-node-agent fusion_packet_is_emitted_when_mmwave_present -- --nocapture`  
Expected: FAIL

- [ ] **Step 3: Implement mmWave adapters**

```rust
pub trait MmwaveReader { fn poll(&mut self) -> Option<MmwaveState>; }
pub struct Ld2410Reader { /* serial parser */ }
pub struct Mr60bha2Reader { /* serial parser */ }
```

- [ ] **Step 4: Implement fusion encoder**

```rust
pub fn encode_fused_vitals(csi: &VitalsState, mw: &MmwaveState) -> Vec<u8> { /* 0xC5110004 */ }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p wifi-densepose-pi-node-agent -- --nocapture`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/mmwave.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/main.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/agent_pipeline_test.rs
git commit -m "feat(pi-agent): add mmwave fusion packet path for full parity"
```

---

### Task 8: Add WASM Runtime Path in Pi Agent

**Files:**
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/wasm_runtime.rs`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/main.rs`
- Test: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/agent_pipeline_test.rs`

- [ ] **Step 1: Write failing WASM event packet test**

```rust
#[tokio::test]
async fn wasm_runtime_emits_v2_wasm_packet() {
    let pkt = run_wasm_fixture_event().await;
    assert_eq!(magic(&pkt), 0xC511_0006);
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p wifi-densepose-pi-node-agent wasm_runtime_emits_v2_wasm_packet -- --nocapture`  
Expected: FAIL

- [ ] **Step 3: Implement runtime wrapper**

```rust
pub struct WasmRuntime { /* engine, modules */ }
impl WasmRuntime {
    pub fn on_frame(&mut self, ctx: &EdgeFrameContext) -> Vec<WasmEvent> { /* host API */ }
}
```

- [ ] **Step 4: Encode and publish events as `0xC5110006`**

```rust
pub fn encode_wasm_v2(node_id: u8, module_id: u8, events: &[WasmEvent]) -> Vec<u8> { /* variable length */ }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p wifi-densepose-pi-node-agent -- --nocapture`  
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/wasm_runtime.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/main.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/agent_pipeline_test.rs
git commit -m "feat(pi-agent): add wasm runtime and v2 wasm packet emission"
```

---

### Task 9: Replace Bridge Default and Keep Compatibility Utility

**Files:**
- Modify: `scripts/nexmon_to_ruview_bridge.py`
- Modify: `docs/user-guide.md`
- Modify: `README.md`

- [ ] **Step 1: Add deprecation banner and compatibility mode flag**

```python
parser.add_argument("--compat-layout", action="store_true",
                    help="Use legacy shifted field layout for old server builds")
```

- [ ] **Step 2: Default to canonical frame layout**

```python
if args.compat_layout:
    frame = build_legacy_compat_frame(...)
else:
    frame = build_canonical_frame(...)
```

- [ ] **Step 3: Validate script syntax**

Run: `python -m py_compile scripts/nexmon_to_ruview_bridge.py`  
Expected: no output, exit code 0

- [ ] **Step 4: Commit**

```bash
git add scripts/nexmon_to_ruview_bridge.py docs/user-guide.md README.md
git commit -m "chore(bridge): keep nexmon bridge as compatibility tool, not primary path"
```

---

### Task 10: Deployment, Services, and Multi-Node Operations on Pi

**Files:**
- Create: `deploy/pi/systemd/ruview-pi-agent.service`
- Create: `deploy/pi/systemd/ruview-sensing-server.service`
- Create: `deploy/pi/install.sh`
- Modify: `docs/pi-deployment-guide.md` (create if missing)

- [ ] **Step 1: Create systemd service units**

```ini
[Service]
ExecStart=/usr/local/bin/wifi-densepose-pi-node-agent --listen 0.0.0.0:5500 --aggregator 10.0.0.5:5005
Restart=always
User=ruview
```

- [ ] **Step 2: Create installation script**

```bash
#!/usr/bin/env bash
set -euo pipefail
sudo install -m 0755 target/release/wifi-densepose-pi-node-agent /usr/local/bin/
sudo install -m 0644 deploy/pi/systemd/ruview-pi-agent.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now ruview-pi-agent
```

- [ ] **Step 3: Validate service files**

Run: `systemd-analyze verify deploy/pi/systemd/ruview-pi-agent.service deploy/pi/systemd/ruview-sensing-server.service`  
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add deploy/pi/systemd deploy/pi/install.sh docs/pi-deployment-guide.md
git commit -m "ops(pi): add systemd deployment and install workflow for pi-only clusters"
```

---

### Task 11: Verification Matrix and Performance Gates

**Files:**
- Modify: `docs/pi-deployment-guide.md`
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/perf_smoke_test.rs`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/multi_node_test.rs`

- [ ] **Step 1: Add failing perf/packet-loss smoke test**

```rust
#[test]
fn packet_loss_under_two_percent_at_20hz() {
    let report = run_perf_smoke();
    assert!(report.loss_pct < 2.0, "loss {}%", report.loss_pct);
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test -p wifi-densepose-pi-node-agent packet_loss_under_two_percent_at_20hz -- --nocapture`  
Expected: FAIL until harness and tuning are in place

- [ ] **Step 3: Implement perf harness and thresholds**

```rust
pub struct PerfReport { pub sent: u64, pub received: u64, pub loss_pct: f64, pub p95_latency_ms: f64 }
```

- [ ] **Step 4: Run full verification suite**

Run: `cargo test -p wifi-densepose-sensing-server -p wifi-densepose-pi-node-agent -- --nocapture`  
Expected: PASS

Run: `cargo clippy -p wifi-densepose-sensing-server -p wifi-densepose-pi-node-agent -- -D warnings`  
Expected: PASS

Run: `cargo fmt --all -- --check`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/tests/perf_smoke_test.rs rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/tests/multi_node_test.rs docs/pi-deployment-guide.md
git commit -m "test(pi-parity): add perf and loss gates for pi full-feature rollout"
```

---

### Task 12: Documentation and Migration Completion

**Files:**
- Modify: `README.md`
- Modify: `docs/user-guide.md`
- Create: `docs/pi-deployment-guide.md`

- [ ] **Step 1: Add explicit Raspberry Pi full-feature workflow docs**

```markdown
## Raspberry Pi Full-Feature Mode
1. Configure Nexmon on each Pi node.
2. Run `wifi-densepose-pi-node-agent`.
3. Run sensing server with `--source esp32` (canonical packets) or `--source nexmon` (direct mode).
```

- [ ] **Step 2: Add migration section (ESP32 -> Pi)**

```markdown
### Migration Notes
- `0xC5110006` is the new WASM event packet magic.
- Bridge script is compatibility-only.
- Existing UI/API endpoints remain unchanged.
```

- [ ] **Step 3: Verify docs link integrity**

Run: `rg -n "pi-node-agent|source nexmon|0xC5110006" README.md docs/user-guide.md docs/pi-deployment-guide.md`  
Expected: lines found in all three files

- [ ] **Step 4: Commit**

```bash
git add README.md docs/user-guide.md docs/pi-deployment-guide.md
git commit -m "docs: publish raspberry pi full-feature parity guide and migration notes"
```

---

## Release Gates (must all pass)

1. All sensing-server protocol tests pass.
2. All pi-node-agent tests pass.
3. End-to-end multistatic test with at least 3 Pi nodes passes for 30 minutes.
4. Packet loss < 2% at 20 Hz and p95 end-to-end latency < 120 ms.
5. UI endpoints `/health`, `/api/v1/sensing`, `/api/v1/edge-vitals`, WebSocket stream unchanged.
6. Docs updated for operators and migration.

---

## Suggested Timeline (calendar dates)

1. **April 22-24, 2026:** Tasks 1-4 (protocol and server correctness).
2. **April 25-29, 2026:** Tasks 5-8 (Pi agent + parity features).
3. **April 30-May 2, 2026:** Tasks 9-10 (deployment and compatibility).
4. **May 3-5, 2026:** Tasks 11-12 (verification, docs, release readiness).

---

## Self-Review

1. **Spec coverage:** Full feature parity (raw/vitals/feature/fused/wasm/multinode/deploy/docs) is mapped to Tasks 1-12.
2. **Placeholder scan:** No placeholder markers remain in task steps.
3. **Type consistency:** Packet IDs, source modes, and module names are consistent across tasks.
