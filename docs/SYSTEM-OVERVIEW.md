# RuView / Wave — Complete System Overview

> **What it is:** a privacy-first **WiFi human-sensing platform**. Cheap WiFi radios
> (ESP32-S3, Raspberry Pi 4 + Nexmon) capture Channel-State-Information (CSI); a
> local Rust **sensing server** turns that raw RF into presence, motion, breathing,
> heart rate, fall events, person count and a 3D pose — **through walls, in the
> dark, with no cameras**. A Tauri **desktop app** ("Wave Desktop") is the control
> plane; a headless **`wave-cli`** scripts everything; and an optional **Frappe/
> ERPNext** cloud backend adds enterprise fleet management, RBAC, and an AI
> clinical-insight pipeline.

This document is the authoritative, code-grounded map of everything the system
does. Where a capability is **present but not fully wired / needs hardware**, it
is marked ⚠️ — nothing here is overclaimed.

---

## 1. Architecture

| Component | Tech | Role |
|-----------|------|------|
| **Wave Desktop** | Tauri v2 (Rust) + React 18/TS (`ui-v2`) | Control plane: onboarding, provisioning, live 3D/medical/security views, user/tenant admin. Auto-starts & manages the sensing server. |
| **Sensing server** | Rust / Axum (`sensing-server`, bundled sidecar) | Ingests CSI over UDP, runs the DSP + neural pipeline, serves REST + WebSocket, hosts the 3D Observatory. |
| **`wave-cli`** | Rust / clap (bundled sidecar, on PATH) | Headless management: provisioning, OTA, users/tenants/licensing, ERP sync, health — everything the app does, scriptable. |
| **ESP32-S3 nodes** | C firmware (ESP-IDF) | WiFi CSI capture, edge DSP, on-device neural inference, TDM mesh, WASM runtime; stream UDP to the hub. |
| **Raspberry Pi 4 nodes** | Nexmon CSI + native Rust agent | BCM43455 CSI capture (primary production path, ADR-090), edge DSP + on-device inference. |
| **Frappe/ERPNext backend** | Python (`ruview_care` app) + MariaDB + Redis | Optional cloud: fleet DocTypes, RBAC, LangGraph AI-insight RQ pipeline, deployment registry, risk alerts. |

**Default ports:** HTTP `4000` · WebSocket `4001` · ESP32 UDP `5005` · Nexmon UDP `5500` · node OTA `8032` · node WASM `8033`.

**End-to-end data flow:**
```
Raw CSI (per-subcarrier amplitude/phase)
  → node edge DSP (biquad filters, FFT, variance, thresholds) → 8 features + vitals
  → [on-device neural refinement, 0xC5110009]  ─UDP 5005/5500→  sensing server
  → hub pipeline (person count, pose tracker, calibration) → node_states roster
  → WebSocket /ws/sensing + REST /api/v1/* (CORS-enabled)
  → React live views  ·  optional HMAC-signed upload → Frappe cloud (AI insights)
```

---

## 2. The Sensing Core — DSP + Neural

**The measurements are produced by classical DSP; a tiny neural model refines them.**

### 2.1 DSP pipeline (the engine, always on)
On raw CSI the server/firmware compute, with no ML:
- **Presence & motion** — thresholds on motion-band power → `active` / `present_still` / `absent`.
- **Breathing & heart rate** — biquad IIR bandpass filters (breathing 0.1–0.5 Hz, heart 0.8–2 Hz) + zero-crossing → BPM.
- **Person count** — eigenvalue occupancy (SVD of the CSI covariance + Marčenko–Pastur threshold) or a hysteresis feature-score heuristic or a subcarrier-correlation min-cut; capped ~10.
- **Fall detection** — phase-acceleration threshold + stillness.
- **Signal quality / adversarial gating**, RSSI, spectral features.
- **3D pose** — a 17-keypoint **Kalman tracker** with re-ID; positions are *derived* from the person count (⚠️ synthetic placement, not a trained CSI→joint model).

### 2.2 On-device neural inference (`wifi-densepose-edge-infer`)
The bundled `ruvnet/wifi-densepose-pretrained` model is a tiny **8→64→128 MLP + BatchNorm + per-room LoRA + sigmoid presence head** (~9,900 MACs, int4 ≈ 4.6 KB). It **re-scores the 8 DSP features** per room (it sits *on top of* the DSP, not on raw CSI). Input order (locked from `scripts/deep-scan.js`): `[presence, motion, breathing, heart_rate, phase_var, persons, fall, rssi]`.

It runs in **three places from one shared Rust crate** (ESP32 C is parity-matched):
- **Hub** — `sensing-server` loads it, runs per node, serves `neural_presence` + `/api/v1/nodes/{id}/inference` (neural vs DSP side-by-side).
- **Pi agent** — runs it on-device, emits an inference packet (`0xC5110009`); hub prefers the node's value (`neural_source: "node"`).
- **ESP32-S3** — C kernel (`inference.c` + generated `model_weights.h`) emits the same packet. ⚠️ Needs flashing a real board to validate.

Honest note: the bundled per-room LoRA was trained on the developer's rooms, and there's no measured accuracy-vs-DSP on arbitrary sites — the `/inference` endpoint exposes the comparison so it can be measured before trusting it. See `docs/plan-on-device-model-inference.md`.

---

## 3. Onboarding (first-run wizard)

Seven steps (`ui-v2/src/components/onboarding/steps/`):
**Welcome** → **Hub Check** (server up? nodes known?) → **Add Node** (auto-scans USB for ESP32s) → **ESP32 Setup** (flash 4-segment bundle → WiFi form with SSID/password/hub-IP prefill + "Scan nearby WiFi" → provision NVS → **serial verify** "Connected to WiFi") **or Pi Setup** (SSH probe → prereqs → **install Nexmon CSI end-to-end**, Ethernet-gated → deploy agent) → **Place Nodes** (drag markers in 3D, or FTM auto-position) → **Calibration** (empty-room field-model baseline).

---

## 4. Authentication & Session

Dual-path (`commands/auth.rs`, `auth/*`, SQLite `wave.db`, JWT):
- **Local users** — SQLite, **Argon2id** password hashing, tenant-scoped, JWT (15-min access + refresh, 12-min auto-refresh), login **rate-limited** (`auth/rate_limiter.rs`).
- **Vendor / super-admin** — `@wave.io` emails validated against a cloud License Server; JWT carries `scope: "global"` (bypasses tenant filtering + license wall).
- **Built-in offline admin** — `admin@wave.io` / `admin` compiled into **all** builds (`auth/super_admin.rs:56-78`). ⚠️ Anyone with the binary is global super-admin; remove before public distribution.
- **Audit trail** — `auth/audit.rs`: every login/CRUD/license/vendor action logged (vendor actions tagged `[VENDOR]`).

---

## 5. RBAC, Users, Tenants & Licensing

### 5.1 RBAC (`commands/users.rs`, `roles.rs`, `auth/rls.rs`, `auth/seed.rs`)
- **Users** — full CRUD + role assignment; creation enforces the license's `max_users`.
- **Roles** — CRUD + per-module permission matrix (`can_read/add/edit/delete/approve` + scope: own-records vs tenant-wide).
- **12 access modules** seeded on first launch: dashboard, network, firmware, edge-modules, pi-nodes, sensing, mesh, provisioning, pose-3d, settings, user-management, tenant-management.
- **Row-level security** — every query is tenant-filtered (`rls::tenant_filter`); super-admins bypass; `check_module_access` does tenant-has-module + role-has-permission.

### 5.2 Licensing & multi-tenancy (`commands/license.rs`, `auth/license.rs`)
- **License activation** (first launch) — validates key with the cloud server (dev-bypass grants an enterprise dev tenant), seeds the tenant, its modules, an Admin role, and the first admin user (with `must_change_password`). Hardware-fingerprint pinned; 7-day offline grace.
- **Tenants** — super-admin CRUD + per-tenant module assignment.

### 5.3 Plan tiers (`plan.rs`, `commands/plan.rs`, ui-v2 plan-store)
**Local · Cloud · Enterprise**, derived from the license type. `require_plan(Cloud|Enterprise)` gates: Cloud Sync (Cloud+), multi-location deployments + WhatsApp alerts (Enterprise). UI shows an **UpgradePrompt**.

---

## 6. Hardware Lifecycle

### 6.1 Discovery / flash / provision (ESP32) — `commands/discovery.rs`, `flash.rs`, `provision.rs`
Network discovery (mDNS/UDP/HTTP-sweep, merged with the streaming roster) · serial-port enumeration (Windows `Get-CimInstance`, catches native-USB VID 303A) · WiFi scan + saved-password read (`netsh`) · host SSID/LAN-IP · firmware release download + multi-segment `esptool` flash with progress · **native NVS provisioning** (golden-tested `csi_cfg` generator) · **serial verify** (resets board, greps boot log for "Connected to WiFi"/"Got IP") · mesh-config generation · NVS read/erase/validate.

### 6.2 OTA — `commands/ota.rs`
Single + **batch** OTA over HTTP `:8032`, **HMAC-SHA256 PSK-signed** firmware, endpoint/version probe.

### 6.3 Raspberry Pi / Nexmon — `commands/pi_node.rs`
SSH probe · prereq install · **end-to-end Nexmon CSI install** (Ethernet-gated, streams logs, auto-waits reboot, installs monitor-mode boot service) · build/deploy the native agent · push config · install/manage systemd service · CSI-health capture.

### 6.4 WASM edge runtime — `commands/wasm.rs`
Upload/list/info/stats/control WASM modules on nodes (`:8033`); nodes run them at runtime (no reflash).

---

## 7. Live Views

- **3D Pose / Observatory** (`pose3d-page.tsx`) — Three.js scene of node markers + tracked skeletons from `/ws/sensing`; **node-disconnect toasts**; hides the pose with a "no active nodes" panel when all nodes are down (data isn't accurate); resumes on reconnect.
- **Node positioning** — manual 3D drag (persisted to `/api/v1/config/node-positions`) or **FTM auto-positioning** (`/api/v1/ranging/*`, MDS solver).
- **Node stats & health** — `/api/v1/nodes` adds `health` (healthy/degraded/stale/offline + reasons) + `fps`; `/api/v1/nodes/{id}/stats` gives fps, detector rate, RSSI, MAC, last-inference age, position.
- **Dashboard** — online/degraded node counts, presence, person count, vitals, server/discovery status.
- **Mesh / Network** — TDM topology graph, discovery table, per-node health, mesh-role editing.

---

## 8. Medical, Security & Vision

- **Medical Hub** (`medical-page.tsx`) — live HR/BR + sparkline history, fall detection, and the **AI-insight report** (risk gauge, trends) from the Frappe LangGraph pipeline (§10).
- **Security Center** (`security-page.tsx`, `use-alert-system.ts`) — armed/disarmed intrusion detection, occupancy vs `crowd_threshold`, over-capacity + intrusion alerts (person-count consistent with the dashboard), WhatsApp dispatch.
- **CSI Vision** (`csi-vision-page.tsx`) — LatentCSI (arXiv:2506.10605): a CSI encoder feeding a Stable-Diffusion v1.5 latent-diffusion pipeline to visualize the RF field.

---

## 9. Model Hub & Bundled Models

- **Bundled pretrained models** shipped in the installer (`resources/models/`, `bundle.resources`): the contrastive encoder (`csi-embed-v2.safetensors`), quantized edge variants (`model-q4/q2/q8.bin`, `csi-embed-v2-int4.bin`), per-node LoRA adapters (`node-1/2.json`), presence head, configs (~165 KB). `list_bundled_models` enumerates them; the **sensing-server + Pi agent embed them** (`include_bytes!`); the **ESP32 compiles them in** (`model_weights.h`). They **are now used** (§2.2) — after a rebuild of the binaries.
- **Model Hub page** (`models-page.tsx`) — HuggingFace search/download UI. ⚠️ Its backend commands (`search_hf_models` etc.) were never merged, so that page's live search is non-functional; it doesn't affect the bundled-model path.
- **Server model management** — RVF load/unload/activate, SONA profiles, LoRA profiles, progressive layers/segments (`/api/v1/model*`, `/models*`).

---

## 10. ERP / Cloud Management Backend (Frappe/ERPNext)

The optional `ruview_care` Frappe app (`services/frappe/`) turns the system into a managed, multi-site enterprise product. **MariaDB + 3× Redis (cache/queue/socketio) + Frappe** via Docker Compose.

### 10.1 DocTypes (data model)
| DocType | Purpose |
|---------|---------|
| **CSI Session** | One sensing session: `hr_mean`, `br_mean`, `presence_ratio`, `csi_snr_db`, `pose_anomalies`, linked to a deployment. |
| **Insight Report** | AI output for a session: `risk_score`, `risk_level` (low/moderate/high/critical), HR/BR classifications, `fall_risk_score`, `trend_direction`, `summary`, `action_items`, `confidence`, `agent_trace_id`. |
| **RuView Deployment** | A physical site: `deployment_id` (UUID from the app), status (Online/Offline/Degraded), `active_risk_level`, geo lat/lon, `tenant_id`, `node_count`, `last_seen`, ERPNext Customer/Project links. |
| **Risk Alert** | Auto-created on high/critical risk; Open→Acknowledged→Resolved lifecycle. |
| **Sensing Node** | Hardware inventory: IP, chip type, health, firmware, MAC. |
| **RuView Settings** (singleton) | OpenAI key, insight model, HMAC `ingest_signing_key`, rate limit. |

### 10.2 RBAC roles
**Enterprise Admin** (full CRUD) · **Clinical Staff** (read + acknowledge alerts) · **Operator** (read/write sessions & nodes) · **Viewer** (read-only).

### 10.3 AI-insight pipeline (LangGraph, RQ background job)
`insight_pipeline.py` — a multi-agent DAG on GPT-4o-mini:
```
parallel_initial (vitals_node ‖ anomaly_node) → clinical_node
  → parallel_assessment (risk_node ‖ trend_node) → synthesis_node → Insight Report
```
`risk_node` scores deterministically (HR/BR abnormal + fall → composite risk → level); high/critical updates the deployment and creates a **Risk Alert**. Triggered by `ingest_csi_session` (auto-enqueues) or `run_insight`.

### 10.4 REST API bridge (`api.py`)
`register_deployment` (upsert + heartbeat) · `ingest_csi_session` (HMAC-validated, stores + enqueues) · `run_insight` · `get_insight_by_session_id` · `get_analytics_trends` · `get_risk_distribution` · `get_deployments_summary`.

### 10.5 Scheduled tasks & dashboard
`update_deployment_heartbeats` (mark Offline after 5 min silence) · `generate_risk_alerts` (hourly) · a **RuView Dashboard** workspace with deployment/alert/session shortcuts and "Deployments by Risk" / "Open Alerts" charts.

### 10.6 Desktop ↔ Frappe (`commands/frappe_*.rs`, `analytics.rs`, `cloud.rs`, `deployment.rs`)
Credentials in the **OS keychain** (`ruview-frappe`), loaded to env on boot; startup **heartbeat** registers the deployment; `run_insight_pipeline` / `get_session_insight` / `get_analytics_trends` / `get_risk_distribution` drive the Medical Hub; consent-gated **HMAC-signed session upload**.

---

## 11. Multi-Location Deployments

`commands/deployment.rs`, `deployment.rs`, `deployments-page.tsx` — each install has a UUID deployment identity (name, location, geo, tenant) in `deployment.json`; `register_deployment` heartbeats to Frappe; `list_deployments` / `get_deployments_aggregate` power an **enterprise fleet dashboard** (total/online/offline/high-risk/avg-risk) — Enterprise-tier, tenant-scoped.

---

## 12. Management CLI (`wave-cli`)

A headless, scriptable superset of the app — bundled as a Tauri sidecar and **added to PATH by the installer** (NSIS post-install / WiX `<Environment>`). clap 4, TTY-aware `table`/`json`/`yaml` output, categorized exit codes, `--yes` for automation. ~30 command groups / ~90 subcommands:

| Group | What it does |
|-------|--------------|
| `server` | start/stop/restart/status/logs the bundled sensing-server. |
| `node` / `sensing` / `pose` / `vitals` | roster, live frame, pose, zones, vitals (hub REST). |
| `serial` / `wifi` | list ports, scan WiFi, host SSID/IP. |
| `esp32` | flash · provision · serial-check · **onboard** (flash→provision→verify→watch) · fetch · verify · erase/read-nvs · chips. |
| `ota` | check · update · **batch** (HMAC-PSK signed). |
| `wasm` | list/upload/start/stop/info/stats/support on nodes. |
| `pi` | probe · service (systemctl) · health (monitor-mode). |
| `monitor` | live terminal dashboard (FPS, presence, vitals, inference latency). |
| `calibrate` / `recording` / `train` / `adaptive` / `ranging` | calibration, session recording, training, adaptive classifier, FTM ranging. |
| `model` | list/bundled/info/active/load/unload/sona/lora/layers/segments. |
| `user` / `role` / `tenant` / `license` / `plan` | full admin against `wave.db` (Argon2id; same DB the app uses). |
| `config` | app settings (`settings.json`). |
| `frappe` / `insight` / `deployment` / `alerts` / `cloud` | ERP: Frappe creds (keychain), LangGraph insight run/get/trends/risk, deployment register/list/aggregate, WhatsApp qr/test/send, cloud config. |
| `path` / `doctor` / `coverage` | PATH install/uninstall, environment self-check, endpoint-coverage audit. |

It talks to **five backends**: hub REST (`:4000`), local process control, direct hardware (esptool/serial/`netsh`/SSH/node HTTP), `wave.db` (SQLite), and Frappe (HTTPS). Secrets live in the OS keychain / env, never in config.

---

## 13. Production, Enterprise & Security

| Capability | What & where |
|------------|--------------|
| **Encryption at rest** | AES-256-GCM `DataEncryptor` (aws-lc-rs, FIPS-capable; 12-byte nonce + 16-byte tag) — `encryption.rs`; **Stronghold** vault for the JWT secret — `auth/vault.rs` (plugin registered at startup); SQLCipher path documented (feature-gated). |
| **Transport / auth hardening** | Sensing-server **JWT Bearer** middleware (REST + WS) — `middleware/auth.rs`; **self-signed TLS** via rcgen — `tls.rs`; ESP32 OTA **HMAC-SHA256 PSK**. |
| **Cloud sync** | Consent-gated, **HMAC-SHA256-signed** session upload, `deployment_id`-tagged — `cloud/uploader.rs`, `commands/cloud.rs`. |
| **Observability** | **Langfuse OTLP** tracing (host + keys) — `commands/telemetry.rs`; `tracing` throughout the server. |
| **Alerting** | **WhatsApp via WHAPI** — QR pair, test, structured alerts — `commands/enterprise.rs` (per-tenant token/number/thresholds). |
| **SSH keys** | PEM-validated private keys in the OS keyring — `commands/security_keys.rs`. |
| **RBAC / tenancy / audit / licensing** | §4–5; `auth/{rls,seed,audit,license,password,rate_limiter}.rs`. |

---

## 14. Packaging & Distribution

- **Windows** `.msi` (WiX) + `.exe` (NSIS) — bundle the app, the **`sensing-server` sidecar**, the **`wave-cli` sidecar** (added to PATH), and the **pretrained models** (`bundle.resources`). Verified via MSI extract.
- **Portable** — the exe + `sensing-server.exe` + `resources/models/` side-by-side.
- **macOS / Linux** — built in CI (`.github/workflows/desktop-release.yml`): macOS `.dmg` (Intel + Apple Silicon), Linux `.AppImage`/`.deb`, on `desktop-v*` tags (free on the public repo). Each job builds both sidecars per-platform.
- ⚠️ **Android** — not supported (would be a separate remote-client app).
- ⚠️ Hardware provisioning (serial/WiFi/flash) is **Windows-centric** (PowerShell/`netsh`/esptool); macOS/Linux builds are monitoring/viewer apps until those are ported.

---

## 15. Honest Gaps

1. **Built-in `admin/admin`** ships in every build — remove before public distribution.
2. **On-device neural inference:** hub + Pi are built & tested; **ESP32 C port needs a real board** to validate; the per-room LoRA is the developer's rooms (measure with `/inference` before trusting).
3. **3D pose skeletons are derived/synthetic**, not a trained CSI→joint model.
4. **Model Hub HF search/download backend is missing** (bundled models are unaffected).
5. **AI insights + cloud sync require the Frappe backend** running/connected.
6. **The running/installed binaries need a rebuild** to include the latest inference + stats endpoints (the source is committed; installers are built on tag).

---

## Appendix — Interfaces

- **Tauri commands (~92, 24 modules):** auth, users, roles, license, plan, discovery, flash, provision, ota, wasm, pi_node, server, settings, models, enterprise, telemetry, analytics, cloud, deployment, frappe_config, frappe_client, security_keys.
- **Sensing-server REST/WS (~52):** `/api/v1/nodes[/{id}/inference|/stats]`, `/sensing/latest`, `/vital-signs`, `/pose/*`, `/calibration/*`, `/recording/*`, `/train/*`, `/adaptive/*`, `/ranging/*`, `/model*`, `/models*`, `/ws/sensing`, `/health*`.
- **UDP packet magics:** `0xC5110001` raw CSI · `…0002` edge vitals · `…0003` feature vector · `…0004` fused vitals · `…0005` compressed · `…0006` feature-state · `…0007` WASM v2 · `…0008` FTM range · **`…0009` on-device inference result**.
- **UI pages (21):** 3D Pose, Overview, CSI Vision, Medical Hub, Security Center, Model Hub, Sensing, Mesh, Network, Flash, Provisioning, OTA, Edge Modules, Pi Nodes, Deployments, Enterprise Settings, User Management, Access Matrix, Tenancy Oversight, System Admin, (Login / License gate).
- **`wave-cli`:** ~30 groups / ~90 subcommands (§12).
