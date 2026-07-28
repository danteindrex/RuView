# Wave Action Plan — ESP32 + Raspberry Pi Dual Support

Verified against the confirmed audit findings. Execution order within each group matters (earlier items unblock later ones).

---

## GROUP 1 — BLOCKERS for basic ESP32 + Pi dual support

### 1.1 Fix Nexmon parsers to match real nexmon_csi firmware layout (2-byte offset bug)
- **Where:** `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/nexmon.rs:14-49` and `rust-port/wifi-densepose-rs/crates/wifi-densepose-pi-node-agent/src/nexmon_capture.rs:12-48`
- **What:** Both parsers assume a 16-byte header with no rssi/fc. Real layout (per `nexmon_csi/src/csi_extractor.c:135-146`): magic u16 [0..2], `rssi` i8 [2], `fc` u8 [3], MAC [4..10], `seqCnt` u16 LE [10..12], `csiconf` [12..14], `chanspec` [14..16], `chip` [16..18], CSI IQ from offset 18. Fix field offsets in both files, use the real rssi byte (delete `estimate_rssi_from_iq` call sites at nexmon.rs:68 and nexmon_capture.rs:116), min-length check >= 22. Add a golden-packet test constructed from the `csi_extractor.c` struct so both parsers validate against firmware, not each other. Without this, one Pi scatters into 8 phantom node IDs and IQ is misaligned — Pi path is garbage.
- **Size:** M

### 1.2 Spawn BOTH UDP listeners (5005 + 5500) in every source mode; fix yield guards
- **Where:** `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs:5023-5046` (spawn match); yield guards at `main.rs:1415` (windows_wifi_task) and `main.rs:4193` (simulated_data_task)
- **What:** In all arms (esp32, nexmon, wifi, simulate), spawn `udp_receiver_task(state.clone(), args.udp_port)` AND `udp_receiver_task(state.clone(), args.nexmon_port)`. The receiver is already source-agnostic (parse_esp32_frame falls back to nexmon at main.rs:666-669). Change both yield guards from `effective_source() == "esp32"` to `matches!(eff.as_str(), "esp32" | "nexmon")`. This is the single change that makes Pi hot-plug and ESP32+Pi coexistence work per ADR-090.
- **Size:** S

### 1.3 Add rv_feature_state (0xC5110006) decoder — stop misparsing ADR-081 feature-state as WASM
- **Where:** `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/esp32_legacy.rs:196` (new decoder), dispatch in `main.rs:3893` udp_receiver_task; reference layout `firmware/esp32-csi-node/main/rv_feature_state.h:30`
- **What:** Firmware sends a 60-byte feature-state packet at 5 Hz that the server parses as WasmOutputV2 (11 garbage WASM events per boot, then silently dropped). Add a decoder keyed on magic 0xC5110006 + len == 60 + IEEE CRC32 over bytes [0..56] == bytes [56..60]; try it BEFORE parse_wasm_output. Broadcast as new WS type `feature_state` and feed presence/motion/vitals into NodeState like edge vitals. (Firmware magic migration to 0xC5110007 goes in Group 3.)
- **Size:** M

### 1.4 Fix hardcoded sample rates in ESP32 raw path (2 Hz / 10 Hz assumptions vs 20-50 Hz reality)
- **Where:** `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs:3981` (`sample_rate_hz = 1000.0/500.0`) and `main.rs:374` (`VitalSignDetector::new(10.0)`)
- **What:** Track measured per-node inter-frame interval (EMA of `Instant` deltas on NodeState), pass the measured rate to `extract_features_from_frame`, and recreate/reparameterize the node's `VitalSignDetector` when the measured rate drifts >20%. Firmware sends up to 50 Hz (csi_collector.c:60 = 20 ms min). Until fixed, breathing/HR from raw ESP32 CSI — the only path for 56-wide nodes — is noise.
- **Size:** M

### 1.5 ui-v2: route WS message types — stop edge packets clobbering `latestUpdate`
- **Where:** `ui-v2/src/lib/sensing-store.ts:130` (setUpdate) and `ui-v2/src/lib/SensingProvider.tsx:51-58` (onmessage)
- **What:** Switch on `update.type`: only `sensing_update` goes to `latestUpdate`; route `edge_vitals`, `wasm_event`, `edge_feature`, `edge_compressed` into dedicated store slots. Today, with default firmware tier=2, dashboard state is clobbered continuously between sensing_updates (blank frames), and no edge data ever renders. Surfacing the new slots in UI pages is Group 2 (item 2.6); the routing itself is a blocker.
- **Size:** S

### 1.6 Fix Tauri production build (frontendDist path + custom-protocol feature)
- **Where:** `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/tauri.conf.json:7-16` and the crate's `Cargo.toml`
- **What:** Add `[features] custom-protocol = ["tauri/custom-protocol"]` to Cargo.toml. Change `frontendDist` to `"../../../../ui-v2/dist"` and both beforeDevCommand/beforeBuildCommand `cwd` to `"../../../../ui-v2"` (crate dir → repo root is 4 levels up). Verify `cargo tauri build` end-to-end and that the release exe renders without a Vite dev server. Without this, no shippable operator console exists (release exe loads dead devUrl :5174).
- **Size:** S

### 1.7 Node position mapping: explicit node_id→position map (Pi node_base=10 breaks index convention)
- **Where:** `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs:2524-2528, 2548-2550` (lookup), `protocol/packet.rs:59` and `main.rs:668` (hardcoded node_base 10)
- **What:** Replace `positions[node_id - 1]` with an explicit map parsed from `--node-positions "1:x,y,z;10:x,y,z"` (keep backward compat: bare `x,y,z;x,y,z` = ids 1..n). Make the hardcoded node_base 10 a CLI arg matching the agent's `--node-base`. Log a warning when a fresh node has no configured position. Required for location_hint / multistatic geometry in any mixed ESP32+Pi mesh.
- **Size:** M

### 1.8 Fresh-install auth story: offline bootstrap or deployed license server
- **Where:** `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/src/auth/super_admin.rs:13` (DEFAULT_LICENSE_SERVER_URL), `src/auth/license.rs:16,74-190`, `src/commands/auth.rs:80-132`, `src/db/seed.rs`
- **What:** Release builds are a brick (no seeded user; license/vendor auth needs unreachable https://license.wave.io). Decide and implement one: (a) first-launch local-admin setup wizard when no license + no users exist (create tenant + admin via seed helpers), or (b) signed offline license files, or (c) deploy the real license server and bake its URL. Also fix the 15-minute hard expiry: issue a refresh token or longer exp for the vendor-login JWT at auth.rs:80. Blocker for shipping the console to any machine that isn't this dev box.
- **Size:** L

---

## GROUP 2 — HIGH-value functionality gaps

### 2.1 Model inference correctness: Goertzel rate 20→10 Hz + window off-by-one
- **Where:** `main.rs:3832` and `main.rs:4106` (infer_pose calls with `20.0`); `pose_inference.rs:258-267` (window build)
- **What:** Pass `10.0` (the trainer's constant, training_api.rs:839) at both call sites; longer term store sample_rate_hz in RVF metadata. Build the feature window from frames strictly PRECEDING the current one: `frame_history.iter().rev().skip(1).take(VARIANCE_WINDOW)` then reverse — matching training_api.rs:647-649. Both bugs bias every z-scored feature fed to the linear head.
- **Size:** S

### 2.2 Model width adapter (192-wide model vs 56/64/128/256 live widths)
- **Where:** `pose_inference.rs:246-254` (hard reject), `main.rs:3827-3833, 4101-4107` (exact-width selection), misleading comment `main.rs:541`
- **What:** Add a width adapter at inference time — sparse interpolation mapping live width to trained width (pattern already exists: ruvector-solver 114→56 in subcarrier.rs) — with an explicit log line. Surface "model loaded but width-incompatible" in `/api/v1/models/active` and model_status. Fix the comment at main.rs:541. Without this, model inference can never activate on ANY real hardware.
- **Size:** M

### 2.3 Fix 0xC5110004 fused-vitals vs WASM ambiguity + parse the mmWave extension
- **Where:** `protocol/esp32_legacy.rs:162, 286-296` (looks_like_legacy_wasm) and `esp32_legacy.rs:152-189` (parser stops at byte 27)
- **What:** (a) In looks_like_legacy_wasm require len EXACTLY == 8+5*event_count, prefer fused-vitals for len==48 when flags byte [5] has only bits 0-3 set, validate module_id < 4 — fixes mmWave presence detection breaking during person-entry. (b) For magic 0xC5110004 + len >= 48, parse bytes 28..47 (mmwave_hr, mmwave_br, mmwave_distance_cm, mmwave_targets, mmwave_confidence, fusion_confidence) into optional Esp32VitalsPacket fields, include in edge_vitals WS JSON — distance is the only ranging signal from an mmWave node and improves location_hint.
- **Size:** M

### 2.4 Edge packets must refresh liveness/source (wasm/feature/compressed branches)
- **Where:** `main.rs:3893-3944` (contrast with vitals path 3715-3718, raw path 3951-3954)
- **What:** In each of the wasm_event/edge_feature/edge_compressed branches: set `s.last_csi_frame = Some(Instant::now())`, flip source (guarding nexmon precedence), update `node_states[node_id].last_frame_time`. Otherwise edge-only nodes report "esp32:offline" and simulate mode injects synthetic data over real traffic.
- **Size:** S

### 2.5 Fall detection: fix posture string mismatch + derive posture on ESP32 path
- **Where:** `ui-v2/src/pages/medical-page.tsx:48`, `ui-v2/src/lib/use-alert-system.ts:47-48`; server `main.rs:1528` (Debug format) and posture=None at `main.rs:1731, 3854, 4125, 4267`
- **What:** Server: serialize PostureClass with `#[serde(rename_all = "snake_case")]`/explicit to_string instead of `{:?}`. UI: compare against the real variant set (update the wrong comments too). ESP32 path: derive posture/fall from model pose_keypoints or bbox aspect ratio so the field isn't permanently None on the deployed hardware. Currently the fall badge, event log, and WhatsApp alert are dead code.
- **Size:** M

### 2.6 Observatory live view: consume real fields (pose_keypoints, location_hint, bbox)
- **Where:** `ui-v2/public/observatory/js/main.js:744-761, 797-814`; plus sync stale bundle from `ui/observatory/js/` (hud-controller.js, scenario-props.js, main.js, figure-pool.js) into `ui-v2/public/observatory/js/`
- **What:** (a) Copy today's HUD/scenario-props live-data fixes from ui/observatory into ui-v2/public/observatory and re-run `npm run build`; add a sync step or declare one tree canonical. (b) Rewrite the live mapper: position from `location_hint`/bbox center, pose from posture/keypoint geometry, skeleton driven by top-level `pose_keypoints` [x,y,z,conf] tuples (COCO-17 index-mapped) — it currently reads `persons[0].position/motion_score/pose`, fields the Rust struct never sends, so the figure is pinned at origin. (c) Add `location_hint?: [number, number]` to WsSensingUpdate in `sensing-store.ts:97-118`. (d) Also route the new edge/feature_state store slots (from 1.5) into medical page vitals and a WASM event log.
- **Size:** L

### 2.7 Desktop lifecycle: settings-aware auto-start, liveness reconciliation, full-config restart
- **Where:** `wifi-densepose-desktop/src/lib.rs:36-49` (auto-start), `src/commands/server.rs:416-471` (server_status), `server.rs:484-527` (restart_server), `src/commands/settings.rs` (extract non-command `load_settings` helper)
- **What:** Three related fixes: (a) auto-start reads `app_data_dir/settings.json` and maps AppSettings → ServerConfig (ports, bind, source, tick_ms, node_positions, model, load/save RVF) with current hardcoded values as fallback; (b) server_status and start_server_impl reconcile with reality via `child.try_wait()` + sysinfo — reset state when dead, surface last exit status so UI can show "crashed"; add a watcher thread after spawn; (c) store `last_config: Option<ServerConfig>` in ServerState at start, use it as restart baseline; replace `std::thread::sleep` with `tokio::time::sleep` (server.rs:527); only clear state after confirmed death; propagate stop errors. Before auto-start, TCP-probe the http port and adopt/skip/kill an orphan.
- **Size:** L

### 2.8 SensingProvider WebSocket hygiene
- **Where:** `ui-v2/src/lib/SensingProvider.tsx:23, 60-64, 71-72`
- **What:** Skip reconnect while `readyState === CONNECTING`; close existing socket before creating a new one; guard onclose/onerror with `if (wsRef.current !== ws) return;`; track the opened URL and reconnect when status.ws_port/bind change; effect cleanup closes the socket (currently survives logout). Also fix pose3d fallback: `ui-v2/src/pages/pose3d-page.tsx:12` — return null when ws_port unknown (let observatory `_autoDetectLive()` run) or use `ws://127.0.0.1:8765/ws/sensing`; update stale probe list in main.js:591-595 (:3000 → :8080).
- **Size:** M

### 2.9 Pi deploy defaults: never loopback aggregator
- **Where:** `ui-v2/src/pages/pi-nodes-page.tsx:24`, `ui-v2/src/pages/settings-page.tsx:53`, `pi-node-agent/src/main.rs:29`
- **What:** Default the deploy dialog aggregator to the desktop machine's LAN IP (local UDP-socket trick) + :5005; validate/warn when aggregator is 127.0.0.1/localhost while the target Pi host differs; agent refuses obvious loopback unless `--allow-loopback`. Current defaults make the Pi stream to its own loopback, silently.
- **Size:** S

### 2.10 probe_nexmon: multi-packet window (mirror today's probe_esp32 fix)
- **Where:** `main.rs:1810-1822`
- **What:** 4096-byte buffer, 5 s deadline, loop packets until parse_nexmon_payload succeeds or deadline expires — same pattern as main.rs:1775-1807.
- **Size:** S

### 2.11 Firmware discovery responder (:5006) — make node registration work
- **Where:** New `firmware/esp32-csi-node/main/discovery_responder.c` (+ CMakeLists.txt), spawned from `main.c` after line 162; protocol spec in `wifi-densepose-desktop/src/commands/discovery.rs:24, 227-279`
- **What:** ~60-line FreeRTOS task binding UDP :5006, answering `WAVE_DISCOVER` with `WAVE_BEACON|<mac>|<node_id>|<version>|esp32s3|node|<tdm_slot>|<tdm_total>` (all fields already in scope in main.c). Optionally register ESP-IDF mdns `_wave._udp`. Design the same socket to accept a future `WAVE_HUB|<ip>|<port>` announcement (see 2.12). Ship in next firmware tag.
- **Size:** M (firmware + release cycle)

### 2.12 DHCP fragility: hub re-announcement + provisioning warnings
- **Where:** `firmware/esp32-csi-node/main/stream_sender.c:33-59`, `scripts/setup-esp32-node.ps1:70-76`, sensing-server (new periodic broadcast)
- **What:** Short term: setup-esp32-node.ps1 prints a "TargetIp must stay stable — set a DHCP reservation" warning. Firmware: reuse the :5006 socket (2.11) to accept `WAVE_HUB|ip|port` broadcasts from sensing-server and update `s_dest_addr` at runtime; server broadcasts periodically when it has zero senders. Both deployed nodes are one DHCP lease rotation from silent total failure.
- **Size:** M

### 2.13 Fix start-wave.ps1 port mismatch
- **Where:** `scripts/start-wave.ps1:32, 39, 42, 44` and comment lines 7-9
- **What:** `--http-port 8080` (or drop flag), health-check/UI URL → :8080, rewrite the comment to name the real conflicts (8765/5005/5500). Probe `http://localhost:8080/health` first and skip with a clear message if the desktop's managed server is running.
- **Size:** S

### 2.14 Desktop provisioning: shell out to the proven provision.py path; fix NVS keys
- **Where:** `wifi-densepose-desktop/src/commands/provision.rs:23, 62-123, 143, 191, 243-357`, `src/commands/discovery.rs:452-456`; ground truth `firmware/esp32-csi-node/main/nvs_config.c:112-268` and `provision.py:44-97`
- **What:** The serial WAVE_NVS protocol has zero firmware counterpart and the key names are wrong (wifi_ssid vs ssid, tdm_total vs tdm_nodes, etc.). Immediate: hide/disable the desktop Provision/read/erase/configure_esp32_wifi buttons so users aren't routed into guaranteed timeouts. Then rewrite provision commands to generate a real NVS partition image with the firmware's `csi_cfg` key names and flash at 0x9000 via espflash-rs (or bundled esptool), port provision.py's #391 wifi-credentials guard, align node numbering to 1-based, and add a cross-check test asserting provision.rs key list == nvs_config.c key list.
- **Size:** L

---

## GROUP 3 — Polish

### 3.1 Tracker/template hygiene
- **Where:** `main.rs:2576-2598, 2659-2724` (dead apply_temporal_smoothing, duplicate in pose.rs:138), `tracker_bridge.rs:177-282`
- **What:** Delete dead smoothing code. Tag model-derived detections (source field on PersonDetection); hold last model pose briefly instead of falling back to the synthetic template when infer_pose flickers None; consider per-node trackers or one designated primary node instead of interleaving both nodes' detections into one global track. **Size:** M

### 3.2 bbox semantics — `main.rs:2630-2641`: use `x: min_x, y: min_y` in person_from_model_keypoints (everything else is top-left). **Size:** S

### 3.3 location_hint robustness — `main.rs:2548, 2557-2564`: reject/warn node_id 0; normalize per-node weights (z-score of node's own recent motion) so vitals-path motion_energy and raw-path band power don't skew the centroid. **Size:** S

### 3.4 Broadcast/source flaps — `main.rs:4312-4335, 1534-1535, 4193`: (a) broadcast_tick_task only re-sends when latest_update older than tick_ms (kills duplicate WS messages); (b) windows_wifi_task re-checks effective_source() after netsh scan before writing s.source; (c) simulated_data_task resets `s.source = "simulated"` when resuming after esp32 offline. **Size:** S

### 3.5 Vitals-path stale inference — `main.rs:3827-3833`: skip infer_pose in the vitals path unless the node's last raw frame is newer than the previous inference (track ns.last_raw_frame_time); stops double-feeding the Kalman tracker with stale keypoints. **Size:** S

### 3.6 Per-node origin labeling — `main.rs:3951-3953`: record producing parser (esp32_legacy / nexmon / pi-agent) on NodeState and expose in /api/v1/nodes so Pi nodes aren't labeled "esp32". **Size:** S

### 3.7 Compressed CSI (0xC5110005): decode or disable — `protocol/esp32_legacy.rs:264-284`, agent `edge_dsp.rs:57-67`: either implement the inverse XOR+RLE codec (fix the sign-lossy i8 packing in the Pi agent first) and feed reconstructed frames through the raw path, or gate off compressed emission at tier 2 in firmware/agent to save bandwidth and lwIP buffers; document 0xC5110005 as stats-only in ADR-090. **Size:** M

### 3.8 Firmware magic migration — move WASM output v2 to 0xC5110007 in `firmware/esp32-csi-node/main/wasm_runtime.h:46` and support both magics server-side (completes 1.3/2.3 permanently; requires firmware release). **Size:** M

### 3.9 setup-esp32-node.ps1 TDM slots — `scripts/setup-esp32-node.ps1:106-110`: add -TdmSlot/-TdmTotal params forwarded to provision.py (default `--tdm-slot ($NodeId-1)`), or print a post-setup hint. Both live nodes currently free-run as slot 0 of 1. **Size:** S

### 3.10 Desktop lifecycle nits — `lib.rs:135-150`: fix the Exit-handler comment (ports 8080/8765/5005/5500); assign child to a Windows Job Object with KILL_ON_JOB_CLOSE so a desktop crash can't orphan the server; consider `taskkill /T /F` for the Windows stop path (graceful phase is currently unix-only, server.rs:333-341). **Size:** S

---

## Suggested execution order

1. **Sprint 1 (server core, no firmware):** 1.2 → 1.1 → 1.3 → 1.4 → 1.7 → 2.1 → 2.4 → 2.10 (all in sensing-server + pi-node-agent; run `cargo test --workspace --no-default-features` after)
2. **Sprint 2 (desktop + UI):** 1.6 → 1.5 → 2.7 → 2.8 → 2.6 → 2.5 → 2.9 → 2.13
3. **Sprint 3 (models + firmware release):** 2.2 → 2.3 → 2.11 → 2.12 → 3.8 (one firmware tag covering discovery responder, hub re-announce, WASM magic) → 2.14
4. **Sprint 4:** 1.8 (auth/licensing decision needed first) + Group 3 remainder

Per CLAUDE.md, after each sprint: full workspace tests (1,031+ pass), `python v1/data/proof/verify.py`, regenerate witness bundle if tests changed, and update CHANGELOG.md.