# Architecture — WiFi-DensePose (Wave)

*Grounded in: `graphify-out/GRAPH_REPORT.md`, `docker/docker-compose.yml`, `rust-port/wifi-densepose-rs/Cargo.toml`, `v1/src/api/main.py`, `ui/index.html`, individual crate `Cargo.toml` and `src/` files. Last updated: 2026-06-20.*

---

## System type

A WiFi-based human pose estimation and vital-sign sensing platform. Physical WiFi signals perturbed by bodies are captured as Channel State Information (CSI) by ESP32 firmware nodes, shipped via UDP to a Rust sensing server, processed through a DSP + neural-network pipeline, and streamed as COCO-17 keypoint poses to a browser-based UI. Parallel Python implementation provides REST API and WebSocket interfaces. The system supports both real ESP32 hardware and simulated CSI sources.

---

## Architecture style

| Style | Evidence |
|-------|----------|
| **Edge → Hub pipeline** | ESP32 firmware sends compressed CSI frames over UDP to the sensing server; no cloud intermediary |
| **Dual-runtime polyglot** | Rust (`sensing-server`) handles real-time DSP/NN; Python FastAPI handles persistence, auth, REST management |
| **Event-driven streaming** | Pose and vital-sign results pushed via WebSocket to UI consumers; no polling |
| **Modular Rust workspace** | 19 workspace crates with strict dependency ordering; `wasm-edge` excluded (no_std); `ruv-neural` is a separate sub-workspace |
| **Multi-source CSI** | `CSI_SOURCE` env var selects ESP32 UDP, host WiFi scan, or simulation — same pipeline, different ingress |
| **No external broker** | Tokio broadcast channel (`tx`) replaces Kafka/Redis for in-process fan-out; Redis optional for Python-side rate limiting/caching |

---

## Component map

| Component | Responsibility | Language/Framework | Datastore | How reached |
|-----------|---------------|-------------------|-----------|-------------|
| **ESP32 firmware** | Capture WiFi CSI, TDM mesh protocol, channel hopping | C (ESP-IDF v5.4) | NVS flash | On-hardware; sends UDP to port 5005 |
| **sensing-server** | UDP CSI ingestion → RuvSense DSP → WiFlow NN → WebSocket broadcast | Rust / Axum 0.7 | In-memory state (RwLock) | HTTP :3000 (REST + UI), WebSocket :3001, UDP :5005 |
| **python-sensing** | REST API (pose/stream/health/auth), WebSocket management, rate limiting | Python / FastAPI | PostgreSQL (primary), SQLite (fallback), Redis (optional) | HTTP :8080, WebSocket :8765 |
| **UI** | Browser frontend — dashboard, live pose visualisation, sensing tab | Vanilla JS / HTML | none | Served from sensing-server :3000 via `ServeDir` |
| **Tauri desktop** (`wifi-densepose-desktop`) | Native desktop wrapper for the UI | Rust / Tauri v2 | same as sensing-server | Local app (macOS/Windows/Linux) |
| **Raspberry Pi node agent** (`wifi-densepose-pi-node-agent`) | Pi-based sensing node with native kernel nexmon CSI | Rust | none | Network node; sends UDP frames to hub |
| **Point cloud** (`wifi-densepose-pointcloud`) | Dense 3-D point cloud from camera depth + RF tomography | Rust | none | CLI binary `wave-pointcloud` |
| **Geo** (`wifi-densepose-geo`) | Satellite tile, DEM, OSM integration; temporal tracking | Rust | none | Library crate |
| **WASM edge** (`wifi-densepose-wasm-edge`) | Lightweight no_std inference on edge MCUs | Rust / WASM | none | `wasm32-unknown-unknown`; excluded from workspace |
| **ruv-neural** | Sub-workspace of 11 neural crates (graph transformer, embeddings, etc.) | Rust | none | Library; built separately |

---

## How the parts connect

```
[ESP32-S3 / Pi Node]
       │ UDP frames (port 5005, compressed CSI)
       ▼
[sensing-server (Rust)]
  ├── udp_receiver_task  ← parses Esp32CompressedPacket / FeaturePacket
  │        │
  │        ▼ passes to RuvSense pipeline:
  │   wifi-densepose-signal: Phase Cleaning (SpotFi + Hampel)
  │        → Fresnel Zone + BVP Extraction
  │        → RuVector (5 crates: attention, mincut, temporal-tensor, solver, attn-mincut)
  │        → RuvSense 6-stage: Multiband → Phase Align → Multistatic → Coherence → Gate → Pose Tracker
  │        → WiFlow NN (TCN + Axial Attention → DensePose + COCO-17)
  │        → AETHER Re-ID Embeddings → Kalman Pose Tracker
  │
  ├── HTTP  :3000  (REST: /api/v1/models, /api/v1/recording, /api/v1/calibration + ServeDir → ui/)
  └── WS    :3001  (ws://host:3001/ws/sensing — sensing state stream)
                │
                ▼
        [Browser UI / Tauri desktop]
                │  parallel WebSocket
                ▼
[python-sensing (FastAPI)]
  ├── HTTP  :8080  /api/v1/{pose,stream,health,auth}
  ├── WS    :8765  WebSocket pose stream (connection_manager)
  ├── Auth  JWT (HS256, 24h expiry, in-memory blacklist)
  ├── Rate  In-memory (Redis recommended but not enforced)
  └── DB    PostgreSQL (primary) → SQLite (fallback) → Redis cache (optional)
```

**S2S auth**: None configured between sensing-server and python-sensing — they are independent services. The sensing-server has no authentication middleware on its REST or WebSocket endpoints.

**Async wiring**: Rust side uses `tokio::sync::broadcast` channel for fan-out from DSP task to WebSocket subscribers. Python side uses FastAPI background tasks + asyncio for WebSocket management.

---

## Data & auth flow

**State storage:**
- Sensing-server: all state is in-process `Arc<RwLock<AppState>>` — ephemeral, lost on restart
- Python API: PostgreSQL for persistent pose history / sessions; Redis for rate-limit counters; SQLite as automatic DB fallback
- ESP32 NVS: WiFi credentials + target-IP stored in flash via provisioning script

**Authentication (Python side only):**
- `POST /api/v1/auth/login` issues HS256 JWT (24h expiry)
- `AuthMiddleware` extracts Bearer token from `Authorization` header or `token` query param
- `TokenBlacklist` invalidates on logout (in-memory, clears every hour — see security notes)
- Feature flags: `ENABLE_AUTHENTICATION=false` by default (dev config) — must be true in production

**Authentication (Rust sensing-server):**
- None — sensing-server endpoints are unauthenticated (see `SECURITY_REVIEW.md` for confirmed findings)

**Multi-tenancy:** None — single-deployment, single-environment model.

---

## Signal processing pipeline (detail)

```
CSI frame (complex amplitudes per subcarrier per antenna pair)
  ↓ SpotFi phase cleaning + Hampel filter (outlier removal)
  ↓ Conjugate multiplication → phase difference
  ↓ RuVector: sparse subcarrier interpolation 114→56 (ruvector-solver)
  ↓ Fresnel Zone model → Body Velocity Profile (BVP)
  ↓ RuvSense multiband fusion (7 subbands)
  ↓ Phase alignment (LO offset correction)
  ↓ Multistatic fusion (attention-weighted over antenna pairs)
  ↓ Coherence scoring (Z-score) → CoherenceGate (Accept/Reject/Recalibrate)
  ↓ WiFlow: TCN (dilations 1,2,4,8) + Axial Self-Attention → DensePose head → COCO-17
  ↓ AETHER contrastive embedding → Kalman pose tracker (identity persistence)
  ↓ Vital signs: IIR bandpass → zero-crossing (breathing) + autocorrelation (heart rate)
  ↓ MERIDIAN domain adaptation: FiLM conditioning + gradient reversal (cross-environment)
  ↓ WebSocket broadcast → UI
```

---

## Build & run

```bash
# Full Rust workspace (tests, no GPU)
cd rust-port/wifi-densepose-rs
cargo test --workspace --no-default-features

# Run sensing server (simulated CSI)
cargo run -p wifi-densepose-sensing-server -- --source simulated

# Docker: Rust sensing server + Python API
docker compose -f docker/docker-compose.yml up

# Python API only
cd v1 && pip install -r requirements.txt
uvicorn src.api.main:app --host 0.0.0.0 --port 8080 --reload

# ESP32 firmware (Windows, ESP-IDF v5.4 — see CLAUDE.md for full subprocess cmd)
# Flash: python [idf_py path] -p COM7 flash
# Provision: python firmware/esp32-csi-node/provision.py --port COM7 --ssid ... --target-ip ...

# WASM edge (excluded from workspace, builds separately)
cargo build -p wifi-densepose-wasm-edge --target wasm32-unknown-unknown --release

# ruv-neural sub-workspace
cd rust-port/wifi-densepose-rs/crates/ruv-neural
cargo build --workspace
```

---

## Key architectural decisions

| ADR | Decision | Status |
|-----|----------|--------|
| ADR-014 | SOTA signal processing (SpotFi, Hampel, Fresnel) | Accepted |
| ADR-015 | MM-Fi + Wi-Pose training datasets | Accepted |
| ADR-016 | RuVector 5-crate training pipeline integration | Accepted |
| ADR-017 | RuVector signal + MAT integration | Proposed |
| ADR-021 | ESP32 CSI-grade vital sign extraction | Accepted |
| ADR-022 | Multi-BSSID WiFi scanning | Accepted |
| ADR-024 | AETHER contrastive CSI embedding | Accepted |
| ADR-027 | MERIDIAN cross-environment domain generalization | Accepted |
| ADR-028 | ESP32 capability audit + witness verification | Accepted |
| ADR-029 | RuvSense multistatic sensing mode | Proposed |
| ADR-030 | RuvSense persistent field model | Proposed |
| ADR-031 | Wave sensing-first RF mode | Proposed |
| ADR-040 | WASM edge crate (`no_std`, excluded from workspace) | Accepted |
| ADR-072 | WiFlow architecture (TCN + axial attention) | Accepted |
| ADR-090 | Pi protocol parity | Accepted |

Full index: `docs/adr/` (83 files, ADR-001 through ADR-090 with some gaps).

---

*See also: `graphify-out/GRAPH_REPORT.md` for knowledge-graph community analysis and surprising cross-cutting connections.*
