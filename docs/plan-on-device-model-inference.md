# Plan — On-Device Model Inference + Node Stats & Health

**Status:** Proposed · **Owner:** TBD · **Depends on:** bundled models (commit `524bf37f`)

## Goal

Make the bundled CSI-embedding model actually **run on each Pi and ESP32**, make its
output **queryable per node** from the hub/UI, and add **real per-node stats &
health**. The model is a small MLP that refines the 8 DSP features the nodes
already compute (see [CAPABILITIES.md](./CAPABILITIES.md) §17 and the analysis
below) — this plan turns it from *bundled-but-inert* into *running on-device*.

### What "the model" is (verified from the weights)
```
x[8]  ─Linear(8→64) W1[64,8]+b1─► BN1 ─ReLU─► Linear(64→128) W2[128,64]+b2 ─► BN2 ─► e[128]
per-node:  e' = e + 2·(loraB·(loraA·e))          # node-N.json, rank-4 room adapt
presence:  p = sigmoid(e'·w_head[128] + b_head)  # presence-head.json
```
≈ 9,900 MACs, int4 weights ≈ 4.6 KB → **fits ESP32-S3 SRAM; trivial on a Pi.**

### The 8 inputs (per model card) — already computed by the DSP
`[presence, motion, breathing, heart_rate, phase_variance, person_count, fall, rssi]`
→ these map to the hub's `FeatureInfo` + vitals + classification + person_count,
and to the ESP32 `edge_processing` / Pi `edge_dsp` outputs. **The model sits on
top of the DSP** — it re-scores/adapts per room, it does not read raw CSI.

### Honest gate
The bundled per-room LoRA (`node-1/2.json`) was trained on the developer's rooms,
and there is **no measured accuracy vs DSP**. So **Phase 1 must demonstrate a real
before/after gain on the user's CSI before we invest in the device ports (Phases
2–3).** Every phase falls back to DSP if inference is unavailable — never worse
than today.

---

## Current state (grounded in the code)

- **DSP produces everything today** (presence, motion, vitals, person count, fall,
  pose); the neural model is bundled but never loaded.
- **`NodeState` already has hooks**: `last_inference_time`, `ema_frame_interval_s`,
  `detector_rate_hz`, `last_raw_frame_time`, `rssi_history`, `mac`, `origin`,
  `last_udp_addr`, `latest_features`. Node stats/health + inference build on these.
- **Packet IDs** (esp32_legacy.rs): raw `0xC5110001`, edge vitals `…0002`, feature
  vector `…0003`, fused `…0004`, compressed `…0005`, feature-state `…0006`,
  WASM v2 `…0007`, FTM `…0008`. **New packets start at `0xC5110009`.**
- **Pi agent is Rust** (`edge_dsp.rs`, `frame_encoder.rs`, `wasm_runtime.rs`) — the
  inference code is **shared** between hub and Pi. Only ESP32 needs a C port.

---

## Phase 0 — Lock the input contract (read, not reverse-engineer)

Read the authoritative training/feature-assembly script (the model card points to
`ruvnet/RuView` scripts, e.g. `deep-scan.js`) to confirm the exact **8-feature
order + normalization**. Output: a documented `FeatureVector8` spec.
*Blocker for correctness — a wrong order/scale = meaningless output.*

---

## Phase 1 — Reference inference on the HUB (Rust, testable now)

The **canonical implementation** the device ports must match bit-for-bit.

1. **New crate `wifi-densepose-edge-infer`** (`no_std`-friendly, no deps): pure-Rust
   forward pass — `Linear`, `BatchNorm(inference)`, `ReLU`, rank-r `LoRA`, sigmoid
   head. Loads weights from `.safetensors` (hub/Pi) or a packed `int4` blob (ESP
   parity test). Unit-tested against a golden input→output vector.
2. **Wire into `sensing-server`** per node: assemble `FeatureVector8` from
   `FeatureInfo`+vitals+classification+person_count; run inference; store
   `neural_presence`, `embedding`, `last_inference_time` in `NodeState`
   (hook already exists). Select the per-room LoRA by `node_id`.
3. **Query surface (REST):** extend `GET /api/v1/nodes` with `neural_presence` +
   `model_version`; add `GET /api/v1/nodes/{id}/inference` (full embedding +
   latency + input vector).
4. **Verification (the gate):** log **neural vs DSP presence/count** agreement over
   a session on real CSI; emit a summary (agreement %, divergences). Ship a
   `--eval-inference` mode. **Proceed to Phase 2 only if this shows value.**

---

## Phase 2 — On-device inference: Raspberry Pi (Rust)

1. **`pi-node-agent/src/inference.rs`** — reuse `wifi-densepose-edge-infer` (same
   crate as the hub → guaranteed identical math). Feed it the `edge_dsp.rs`
   feature vector.
2. **Model delivery:** bundle the model into the agent deploy (the desktop already
   ships `resources/models/`); select the room LoRA by node id; allow override via
   `pi_node_push_config`.
3. **`frame_encoder.rs`:** emit a new **inference-result packet `0xC5110009`** —
   `{node_id, model_version, presence_q, embedding (optional, int8), latency_us}`.
4. **Hub:** parse `…0009` into `NodeState` → same REST surface as Phase 1, now
   sourced on-device. "Query the model from the Pi" = read `/api/v1/nodes/{id}/inference`.

---

## Phase 3 — On-device inference: ESP32-S3 (C firmware)

1. **`firmware/esp32-csi-node/main/inference.c`** — int4 MLP kernel (8→64→128 + BN +
   ReLU + LoRA + sigmoid), consuming the features `edge_processing` already emits.
   Numerically validated against `wifi-densepose-edge-infer` (int4 vs fp32 parity).
2. **Weight delivery:** embed `csi-embed-v2-int4.bin` as a generated `const` header
   at build time (4.6 KB), **or** provision to NVS/SPIFFS + load. Per-room LoRA via
   NVS (`node-N` adapter, tiny).
3. **Emit the same `0xC5110009`** inference packet; hub parses identically.
4. **Hardware acceptance:** flash a real ESP32-S3, confirm on-device presence tracks
   the hub reference within tolerance, measure added CPU/heap/latency.

---

## Phase 4 — Node stats & health (both targets)

1. **Node side — new stats/health packet `0xC511000A`** emitted every N seconds:
   `{node_id, uptime_s, fps_raw_csi, packets_sent, inference_count, infer_latency_us,
     free_heap (ESP) / rss_kb (Pi), wifi_rssi, temp_c?, fw_version, error_flags}`.
   ESP32: `esp_get_free_heap_size`, uptime, WiFi RSSI, temp sensor. Pi: `/proc` +
   agent counters.
2. **Hub:** extend `NodeState` + `GET /api/v1/nodes` with the stats; add
   `GET /api/v1/nodes/{id}/stats`. Compute a **health status** with reasons:
   `healthy | degraded | stale | offline | error` derived from fps (below expected
   rate), inference latency, no-CSI timeout, low heap, error_flags. (`last_seen_ms`,
   `ema_frame_interval_s`, `detector_rate_hz` already exist to seed this.)
3. **UI:** per-node stats/health cards on **Pi Nodes** / **Network** / **Sensing**
   pages (fps, uptime, RSSI, heap/mem, inference rate + latency, health chip with
   reason on hover). The 3D-pose disconnect notice already consumes roster health;
   extend it to surface "degraded" too.

---

## Cross-cutting

- **"Query the model from each node" contract:** each node runs inference locally
  and reports via `0xC5110009`; the hub exposes it at `/api/v1/nodes/{id}/inference`
  and inline on `/api/v1/nodes`. One contract, three producers (hub-reference,
  Pi, ESP32).
- **Shared math:** hub + Pi use the one `wifi-densepose-edge-infer` crate; ESP32 C
  is parity-tested against it, so all three agree.
- **Fallback everywhere:** stale/absent inference ⇒ fall back to DSP; health flags
  say so. The system is never worse than the current DSP-only baseline.
- **Per-room adaptation (later):** add a "learn/adapt this room's LoRA" flow so the
  benefit transfers to new deployments (the bundled adapters are the dev's rooms).

## Verification per phase

| Phase | Gate |
|-------|------|
| 0 | Documented 8-feature order + normalization from the training script. |
| 1 | `edge-infer` unit test matches golden vector; hub **before/after** eval shows measured gain on real CSI. |
| 2 | Pi on-device inference matches hub reference within tolerance; `0009` round-trips. |
| 3 | ESP32 int4 vs fp32 parity; **hardware acceptance** on a real board; CPU/heap/latency budget met. |
| 4 | Stats/health packet round-trips; health states transition correctly (unplug a node → degraded → stale → offline); UI cards render. |

## Risks

- **Input contract** (Phase 0 mitigates) — wrong feature order/scale = garbage.
- **LoRA not tuned to the user's room** — may show no gain until re-adapted; Phase 1
  gate catches this before device work.
- **int4 accuracy** on ESP32 vs fp32 — parity test in Phase 3.
- **Packet ID allocation** `0xC5110009`/`000A` — confirm no firmware collision.
- **Node compute budget** — ESP32 already runs CSI DSP + TDM + WASM; measure added
  load before committing the firmware path.

## Rollout order (each independently shippable)

`Phase 0 → Phase 1 (+gate) → Phase 4 (stats/health, independent of inference) →
Phase 2 (Pi) → Phase 3 (ESP32)`.

Phase 4 (node stats/health) has no dependency on the model and can ship first if
that's the priority.
