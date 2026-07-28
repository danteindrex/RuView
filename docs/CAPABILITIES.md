# Wave Desktop / Wave — Full System Capabilities

> WiFi Channel-State-Information (CSI) human sensing platform. A Tauri v2 desktop
> app ("Wave Desktop") drives a local sensing server, provisions ESP32-S3 and
> Raspberry Pi sensing nodes, renders live 3D pose / vitals / occupancy, and
> optionally syncs to a Frappe/ERPNext cloud backend for enterprise management
> and AI insights.
>
> This document maps **every capability** in the system, grouped by user journey
> (onboarding → auth → user management → hardware → sensing → analytics →
> enterprise). Where a capability is **present but not yet wired end-to-end**, it
> is marked ⚠️ with an honest note — nothing here is overclaimed.

- **Version:** Wave Desktop `0.4.4`
- **Surface:** 92 Tauri commands (24 modules) · 50 sensing-server HTTP/WS routes · 21 UI pages · 7-step onboarding wizard
- **Default login (all builds):** `admin@wave.io` / `admin` (built-in offline super-admin — see [Security notes](#20-security-notes--honest-gaps))

---

## 1. System Architecture

| Component | Tech | Role |
|-----------|------|------|
| **Wave Desktop** | Tauri v2 (Rust) + React 18/TS (`ui-v2`) | Desktop control plane: onboarding, provisioning, live views, user/tenant admin. Auto-starts and manages the sensing server. |
| **Sensing server** | Rust / Axum (`sensing-server.exe`, bundled sidecar) | Ingests CSI over UDP, runs the signal-processing pipeline, serves REST + WebSocket, hosts the legacy Observatory 3D UI. |
| **ESP32-S3 nodes** | C firmware (ESP-IDF) | WiFi CSI capture, edge DSP, TDM mesh, WASM runtime; stream UDP to the hub. |
| **Raspberry Pi nodes** | Nexmon CSI + native agent | BCM43455 CSI capture (primary production path, ADR-090). |
| **Frappe/ERPNext backend** | Python (`wave_care` app) | Optional cloud: management DocTypes, RBAC, LangGraph AI-insight RQ pipeline, deployment registry. |

**Ports (defaults):** HTTP `4000` · WebSocket `4001` · ESP32 UDP `5005` · Nexmon UDP `5500`.

**Live data flow:** node → UDP `5005/5500` → sensing-server parse & pipeline → `node_states` roster + `sensing_update` frames → WebSocket `/ws/sensing` → React live views. REST (`/api/v1/*`) is CORS-enabled so the Tauri webview can read the roster, calibration, models, etc.

---

## 2. Onboarding (first-run wizard)

Auto-opens on first reach of the dashboard (`onboarding_complete` flag in settings). Seven steps in `ui-v2/src/components/onboarding/steps/`:

| Step | Capability |
|------|-----------|
| **Welcome** | Intro + what you'll need (a node, 2.4 GHz WiFi). |
| **Hub Check** | Confirms the sensing server is up and reachable; shows how many nodes are already known. |
| **Add Node** | Auto-scans USB for connected ESP32-class boards on entry; routes to ESP32 (USB) or Pi (network/SSH) setup. |
| **ESP32 Setup** | Full ESP32-S3 bring-up over USB: serial-port pick → flash 4-segment firmware bundle (live progress) → WiFi form → provision NVS → **serial verify** ("joined WiFi? got IP?") → watch roster. Prefills SSID + Hub IP + **saved WiFi password** from the host; **Scan nearby WiFi** dropdown (2.4 GHz only, saved networks auto-fill password); blocks blank password on secured networks. |
| **Pi Setup** | Full Raspberry Pi bring-up over SSH: probe → prereqs → **install Nexmon CSI end-to-end** (Ethernet-gated, streams logs, auto-waits reboot, installs boot service) → deploy agent → start. |
| **Place Nodes (3D)** | Drag node markers into the room in 3D to set positions; FTM auto-positioning available as an alternative. |
| **Calibration** | Empty-room calibration (field-model baseline) so occupancy/eigenvalue counting has a reference. |

---

## 3. Authentication & Session

Module: `commands/auth.rs`, `auth/*`. SQLite (`wave.db`) with JWT.

| Command | Capability |
|---------|-----------|
| `login` | Dual-auth: **local tenant users** (SQLite) **or vendor super-admin** (`@wave.io`, cloud license server — with built-in offline `admin@wave.io`/`admin` fallback in every build). |
| `logout` / `refresh_token` | Session lifecycle; JWT refresh with auto-timer in the UI. |
| `get_current_user` | Restore session on launch. |
| `change_password` | Self-service password change. |

- **Rate limiting** on login (`auth/rate_limiter.rs`).
- **Scope model:** super-admin has `scope: "global"` (bypasses tenant filtering and the license wall); regular users are tenant-scoped and require a valid license.
- **JWT secret** auto-generated & persisted; access modules seeded on first boot.

---

## 4. User Management (RBAC)

Modules: `commands/users.rs`, `commands/roles.rs`, `hooks/use-permissions.ts`. Pages: **User Management**, **Access Matrix**.

| Command | Capability |
|---------|-----------|
| `list_users` / `get_user` / `create_user` / `update_user` / `delete_user` | Full user CRUD. |
| `assign_user_role` | Attach a role to a user. |
| `list_roles` / `get_role` / `create_role` / `delete_role` | Role CRUD. |
| `set_role_permissions` | Per-module permission grants for a role. |
| `list_modules` / `list_tenant_modules` | Enumerate access modules (global + per-tenant). |

**Access modules** (drive page visibility via `use-permissions`): `dashboard`, `sensing`, `pose-3d`, `mesh`, `network`, `firmware`, `provisioning`, `pi-nodes`, `edge-modules`, `settings`, `user-management`, `tenant-management` (plus `mod-*` grant records). Super-admins see everything; other users see only permitted sections.

---

## 5. Licensing & Multi-Tenancy

Module: `commands/license.rs`. Pages: **Tenancy Oversight**, license activation (first-launch).

| Command | Capability |
|---------|-----------|
| `activate_license` | Activate a license key (cloud validation in release; dev bypass grants an "enterprise" dev tenant). |
| `license_status` | Current license / tenant / allowed modules / limits. |
| `list_tenants` / `create_tenant` / `delete_tenant` | Tenant CRUD (super-admin). |
| `assign_tenant_modules` | Grant/revoke modules per tenant. |

### Plan tiers (`commands/plan.rs`, `plan-store`)
`get_plan_tier` → **Local · Cloud · Enterprise**. Commands can be gated with `require_plan(Cloud)` / `require_plan(Enterprise)`. The UI shows an **UpgradePrompt** and gates Cloud Sync + AI Insights behind the appropriate tier.

---

## 6. Node Discovery, Flashing & Provisioning (ESP32)

Modules: `commands/discovery.rs`, `commands/flash.rs`, `commands/provision.rs`. Pages: **Network**, **Flash**, **Provisioning**.

| Command | Capability |
|---------|-----------|
| `discover_nodes` | Network discovery of nodes (mDNS/UDP probe/HTTP sweep) → `DiscoveredNode[]`. Merged with the streaming roster so streaming nodes always count. |
| `list_serial_ports` | Enumerate USB serial devices (Windows: `Get-CimInstance` — catches native-USB VID 303A boards `tokio_serial` misses). |
| `host_network_info` | The PC's current SSID + LAN IP (prefills provisioning). |
| `scan_wifi_networks` | Nearby WiFi (2.4 GHz flag, saved, connected, open) via `netsh`. |
| `wifi_saved_password` | Read the saved key for an SSID from the Windows profile. |
| `configure_esp32_wifi` | Serial WiFi config helper. |
| `check_espflash` / `supported_chips` | Toolchain + chip metadata. |
| `fetch_firmware_release` | Download & cache the 4 release binaries (bootloader / partition-table / ota_data / app) from the public GitHub release. |
| `flash_firmware` / `flash_firmware_bundle` | Single-image or multi-segment flash via `esptool` with live progress. |
| `flash_progress` | Poll flash progress. |
| `verify_firmware` | Hash-verify flashed firmware. |
| `provision_esp32_nvs` | Native Rust NVS generator writes the `csi_cfg` namespace (ssid/password/target_ip/target_port/node_id) — byte-for-byte golden-tested against ESP-IDF's tool. |
| `provision_node` / `read_nvs` / `erase_nvs` / `validate_config` | Legacy serial provisioning, NVS read/erase, config validation. |
| `esp32_serial_check` | After provisioning, resets the board and scans the serial boot log for "Connected to WiFi" + "Got IP" — splits failures into *creds/5 GHz* vs *network path* (firewall/hub-IP). |
| `generate_mesh_configs` | Auto-generate TDM mesh configs for N nodes. |

---

## 7. OTA Updates

Module: `commands/ota.rs`. Page: **OTA**.

| Command | Capability |
|---------|-----------|
| `ota_update` | Push firmware to a node over the network (PSK-authenticated). |
| `batch_ota_update` | Roll out to many nodes (strategy + concurrency). |
| `check_ota_endpoint` | Probe a node's OTA endpoint / current version / PSK requirement. |

---

## 8. Raspberry Pi / Nexmon

Module: `commands/pi_node.rs`. Page: **Pi Nodes**.

| Command | Capability |
|---------|-----------|
| `pi_node_probe` | SSH reachability + board/kernel info. |
| `pi_node_check_prereqs` | Verify/install apt prerequisites. |
| `pi_node_install_nexmon` | **End-to-end Nexmon CSI install** over SSH: Ethernet-safety-gated, streams logs (`pi-nexmon-progress`), auto-waits the reboot, runs post-reboot verify, installs a monitor-mode boot service. |
| `pi_node_build_agent` / `pi_node_deploy_binary` | Build & deploy the native Pi node agent. |
| `pi_node_push_config` / `pi_node_install_service` / `pi_node_service` | Push config, install/enable systemd service, start/stop/restart/status. |
| `pi_node_csi_health` | Confirm CSI is actually flowing from the Pi. |

---

## 9. Sensing Server & Live Pipeline

Binary: `sensing-server` (bundled sidecar). Managed by `commands/server.rs`. Page: **Sensing**.

| Command | Capability |
|---------|-----------|
| `start_server` / `stop_server` / `restart_server` | Manage the sensing server (auto-started on app launch; adopts an already-running server on the port). |
| `server_status` | Running state, ports, source, PID, memory/CPU, uptime. |
| `server_logs` | Tail stdout/stderr. |

**Data sources:** `esp32` · `nexmon` · `wifi` (PC adapter scan) · `simulate`. The UDP listeners for `5005`/`5500` are always bound, so any streaming node registers regardless of source.

**Pipeline (signal-processing, runs by default):** presence & motion classification, EMA-smoothed **person counting** (field-model eigenvalue occupancy or score/hysteresis heuristic, capped), **vital signs** (breathing/heart rate), posture, signal-quality scoring, adversarial/quality gating, and a 17-keypoint **Kalman pose tracker** with re-ID (persons array bounded to the physics estimate).

**Server REST/WS endpoints** (selected of 50):

| Endpoint | Purpose |
|----------|---------|
| `GET /api/v1/nodes` | Per-node roster: status (active/stale), RSSI, motion, person_count, origin, position. |
| `GET /api/v1/sensing/latest` | Latest full `sensing_update`. |
| `GET /api/v1/vital-signs` · `/edge-vitals` | Vitals (server + edge packets). |
| `GET /api/v1/pose/current` · `/pose/stats` · `/pose/zones/summary` | Pose + zone occupancy. |
| `WS /ws/sensing` · `/api/v1/stream/pose` | Live streams. |
| `POST /api/v1/calibration/start\|stop` · `GET /status` | Empty-room field-model calibration. |
| `GET/POST /api/v1/recording/*` | Record / list / delete CSI sessions. |
| `POST /api/v1/train/start\|stop` · `GET /status` | Training pipeline. |
| `POST /api/v1/adaptive/train\|unload` · `GET /status` | Adaptive classifier. |
| `/api/v1/model/*` · `/models/*` · `/model/sona/*` · `/models/lora/*` | RVF model info, load/unload/activate, SONA profiles, LoRA adapters, progressive layers/segments. |
| `POST /api/v1/ranging/run\|apply` · `GET /status` | **FTM auto-positioning** orchestration. |
| `GET/PUT /api/v1/config/node-positions` | Persisted 3D node placement map. |

---

## 10. 3D Pose View / Observatory

Page: **3D Pose** (`pose3d-page.tsx` → Observatory Three.js host).

- Live 3D scene of node markers + tracked person skeletons, fed by `/ws/sensing`.
- **Node-disconnect notifications:** polls the roster; a transient toast fires when a previously-online node goes stale/drops.
- **Data-integrity guard:** when **all** nodes are down, the pose is covered with a "no active sensing nodes" panel (the data isn't accurate without a node) and resumes automatically on reconnect.
- Theme-aware (light default).

---

## 11. Node Positioning

- **Manual 3D placement:** drag markers in the Place-Nodes step / pose view → persisted via `/api/v1/config/node-positions`.
- **FTM auto-positioning:** 802.11 FTM ranging (`/api/v1/ranging/*`) with a classical MDS solver derives node coordinates automatically; 3D view is the fallback / correspondence to the room.

---

## 12. WASM Edge Runtime

Module: `commands/wasm.rs`. Page: **Edge Modules** (`modules-page.tsx`).

| Command | Capability |
|---------|-----------|
| `check_wasm_support` | Node's WASM capability (max modules, memory limit, signature verify). |
| `wasm_upload` | Push a WASM module to a node (optional auto-start). |
| `wasm_list` / `wasm_info` / `wasm_stats` | Inventory, per-module detail, runtime stats. |
| `wasm_control` | Start/stop/reset a module. |

Nodes run uploaded WASM modules at runtime (loaded into slots by the firmware `wasm_runtime`), enabling edge logic updates without reflashing.

---

## 13. Mesh & Network

Pages: **Mesh**, **Network**. TDM mesh topology visualization, per-node online/health, channel/slot layout, and network discovery controls. Node counts reflect the merged discovery + streaming roster.

---

## 14. Medical Hub & AI Insights

Page: **Medical Hub** (`medical-page.tsx`). Modules: `commands/analytics.rs`, Frappe RQ pipeline.

- Live **vitals** (heart/respiration) and **fall detection** (`posture == lying_down` or edge `fall_detected`).
- **AI insight pipeline** (`run_insight_pipeline`, `get_session_insight`, `get_analytics_trends`, `get_risk_distribution`): a **LangGraph 6-agent** pipeline (parallel vitals + anomaly → clinical interpretation → risk scoring → trend analysis → synthesis) run as a **Frappe RQ background job**; the UI polls async and maps `InsightResult`s into a dashboard with a risk gauge and trend charts. *(Requires the Frappe backend connected — see §18.)*

---

## 15. Security Center

Page: **Security Center** (`security-page.tsx`), `use-alert-system.ts`.

- **Occupancy** card (person count vs configured `crowd_threshold`) with over-capacity warning.
- **Presence / intrusion:** armed/disarmed mode; intrusion alert on motion while armed.
- **Alerts:** crowd-density and intrusion alerts (person-count source consistent with the dashboard). Privacy-first — RF only, no cameras.

---

## 16. CSI Vision (LatentCSI)

Page: **CSI Vision** (`csi-vision-page.tsx`). Feature: `latentcsi`.

CSI→image generation (LatentCSI, arXiv:2506.10605) — a CSI encoder feeding a Stable-Diffusion v1.5 latent-diffusion pipeline (vision service) to visualize what the RF field "sees." UI tab for generation.

---

## 17. Model Hub & Bundled Models

Page: **Model Hub** (`models-page.tsx`). Module: `commands/models.rs`.

- **Bundled pretrained models** (shipped in the installer, `resources/models/`): `ruvnet/wifi-densepose-pretrained` — contrastive CSI encoder (`model.safetensors`), quantized edge variants (`model-q4/q2/q8.bin`, `csi-embed-v2-int4.bin`), per-node LoRA adapters, configs (~165 KB). `list_bundled_models` enumerates them offline.
- ⚠️ **Not loaded/used on startup** — the pipeline runs on classical signal processing; the bundled weights are present but inert (loading them needs a safetensors→RVF loader + auto-start wiring).
- ⚠️ **HF search/download UI is non-functional** — `search_hf_models` / `download_hf_model` / `list_local_models` / `delete_local_model` were never merged into the backend, so those buttons error. (Bundling does not depend on them.)

---

## 18. Deployments (Multi-Location) & Cloud / Frappe Backend

Modules: `commands/deployment.rs`, `commands/cloud.rs`, `commands/frappe_config.rs`, `commands/frappe_client.rs`. Page: **Deployments**.

| Command | Capability |
|---------|-----------|
| `register_deployment` / `get_deployment_info` / `set_deployment_info` | This hub's deployment identity (UUID + metadata). |
| `list_deployments` / `get_deployments_aggregate` | Enterprise multi-location roster + aggregated status (deployment cards dashboard). |
| `get_cloud_config` / `set_consent` / `upload_sensing_session` | Cloud sync: consent gate + upload of a sensing session (deployment_id tagged), HMAC-signed. |
| `get_frappe_config` / `set_frappe_config` | Frappe/ERPNext connection (stored via keyring); startup heartbeat to the Frappe API. |

**Frappe/ERPNext backend** (`wave_care` app): 5 DocTypes, RBAC roles, REST bridge, scheduled tasks, dashboard workspace, and the LangGraph insight RQ pipeline (`ingest_csi_session` + `run_insight`). Replaces the earlier FastAPI bridge. Docker Compose provided.

---

## 19. Enterprise Settings, Observability & Data Protection

| Area | Capability |
|------|-----------|
| **Enterprise settings** (`commands/enterprise.rs`, **Enterprise Settings** page) | Settings CRUD; **WhatsApp alerts** via WHAPI — `whapi_get_qr` (pair), `whapi_send_test`, `whapi_send_alert`. Cloud Backend (Frappe) connection section. |
| **Observability** (`commands/telemetry.rs`) | **Langfuse OTLP tracing** — `get/set_langfuse_config`; sensing server + Python RAG instrumented; settings UI. |
| **Encryption at rest** | **AES-256-GCM** `DataEncryptor` (aws-lc-rs, FIPS 140-3), **Stronghold** vault for secrets (registered at startup), SQLCipher migration docs. |
| **Transport & auth hardening** | Sensing-server **Bearer auth** middleware + TLS module; fixed ESP32 OTA PSK bypass. |
| **SSH keys** (`commands/security_keys.rs`) | SSH key admin UI (for Pi node access). |
| **System Admin** (**System Admin** page) | Node table, server logs, diagnostics, low-level controls. |
| **Settings** (`commands/settings.rs`, **Enterprise Settings**) | Ports, bind address, data source, tick, model/RVF/node-position paths, Pi-agent config, OTA PSK, theme (light default), discovery interval. |

---

## 20. Packaging & Distribution

- **Windows:** `.msi` (WiX) + `.exe` (NSIS) installers — bundle the app, the `sensing-server` **sidecar** (`externalBin`), and the **pretrained models** (`bundle.resources`). Verified via MSI extract.
- **Portable:** `Wave-Desktop.exe` + `sensing-server.exe` + `resources/models/` side-by-side.
- **macOS / Linux:** built in CI (`.github/workflows/desktop-release.yml`) — macOS `.dmg` (Intel + Apple Silicon), Linux `.AppImage`/`.deb` — on `desktop-v*` tags or manual dispatch (free on the public repo). Each CI job builds the sidecar per-platform.
- **Android:** not supported (would be a separate remote-client app; the desktop's process-spawning/serial/flash model doesn't map to Android's sandbox).

---

## 21. Security Notes & Honest Gaps

These are deliberate call-outs so the doc reflects reality, not marketing:

1. **Built-in admin in every build** — `admin@wave.io` / `admin` is compiled into all builds (debug *and* release) as an offline vendor-admin fallback. Anyone with the binary can log in as global super-admin. **Remove/change before public distribution.**
2. **Bundled models are inert** — shipped but not loaded; sensing runs on DSP, not the neural nets (§17).
3. **Model Hub HF backend missing** — search/download commands aren't implemented (§17).
4. **No on-device ESP32 inference** — the firmware has a WASM runtime but no native model inference; the quantized edge weights can't run on the MCU without new firmware work.
5. **Hardware provisioning is Windows-centric** — serial discovery, WiFi scan, and host-network info shell out to PowerShell/`netsh`; macOS/Linux builds work as **monitoring/viewer** apps until those are ported.
6. **AI insights / cloud features require the Frappe backend** connected; without it, the Medical Hub insight pipeline and cloud sync are inactive.

---

## Appendix A — Tauri command index (92 commands, 24 modules)

`auth` (login, logout, refresh_token, get_current_user, change_password) · `users` (list/get/create/update/delete, assign_user_role) · `roles` (list/get/create/delete, set_role_permissions, list_modules, list_tenant_modules) · `license` (activate_license, license_status, list/create/delete_tenant, assign_tenant_modules) · `plan` (get_plan_tier) · `discovery` (discover_nodes, list_serial_ports, configure_esp32_wifi, host_network_info, scan_wifi_networks, wifi_saved_password) · `flash` (flash_firmware, flash_firmware_bundle, flash_progress, verify_firmware, check_espflash, supported_chips, fetch_firmware_release) · `provision` (provision_node, provision_esp32_nvs, esp32_serial_check, read_nvs, erase_nvs, validate_config, generate_mesh_configs) · `ota` (ota_update, batch_ota_update, check_ota_endpoint) · `wasm` (list, upload, control, info, stats, check_wasm_support) · `pi_node` (probe, build_agent, deploy_binary, check_prereqs, csi_health, push_config, install_service, service, install_nexmon) · `server` (start, stop, status, restart, logs) · `settings` (get, save) · `models` (list_bundled_models) · `enterprise` (get/save_enterprise_settings, whapi_get_qr/send_test/send_alert) · `telemetry` (get/set_langfuse_config) · `analytics` (run_insight_pipeline, get_session_insight, get_analytics_trends, get_risk_distribution) · `cloud` (get_cloud_config, set_consent, upload_sensing_session) · `deployment` (register, get/set_deployment_info, list, get_deployments_aggregate) · `frappe_config` (get/set_frappe_config) · `security_keys` (SSH key admin).

## Appendix B — UI pages (21)

3D Pose · Overview (dashboard) · CSI Vision · Medical Hub · Security Center · Model Hub · Sensing · Mesh · Network · Flash · Provisioning · OTA · Edge Modules · Pi Nodes · Deployments · Enterprise Settings · User Management · Access Matrix · Tenancy Oversight · System Admin · (Login / License activation gate).
