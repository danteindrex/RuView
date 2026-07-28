# RuView Management CLI — Design & Implementation Plan

**Binary:** `wifi-densepose` (alias `ruview`) · **Crate:** `wifi-densepose-cli` · **Status:** Proposed

> Goal: a single, scriptable command-line tool — **bundled in the installer** —
> that can manage **everything** the desktop app and sensing server can: node
> onboarding, flashing, provisioning, live sensing, calibration, OTA, Pi setup,
> models, users/tenants/licensing, deployments, cloud, and diagnostics — headless
> and automation-friendly.

Today the CLI only has `mat` (disaster tool) + `version`. This plan takes it to a
full control plane. It is organized as you asked: **(1)** the plan, **(2)** the
complete backend endpoint inventory, **(3)** user flows for every use case,
**(4)** web-researched best practices and a phased build.

---

## 1. Design principles (from research)

Grounded in the [Command Line Interface Guidelines (clig.dev)](https://clig.dev/),
the [cli-guidelines repo](https://github.com/cli-guidelines/cli-guidelines), and
patterns from `kubectl` / `gh` / `docker` / `stripe`:

| Principle | How we apply it |
|-----------|-----------------|
| **Noun-verb structure** | `ruview <resource> <action>`: `ruview node list`, `ruview esp32 flash`, `ruview server start`. Two-level for rich objects (docker-style). |
| **Human vs machine output** | Detect TTY: pretty tables interactively; `--output json\|yaml\|table` + `--jsonpath` for scripts (kubectl/gh). Primary result → stdout, logs → stderr. |
| **Consistent flags** | Global `--output/-o`, `--url`, `--quiet/-q`, `--verbose/-v`, `--no-color`, `--yes/-y`, `--dry-run` everywhere. Full + short forms. |
| **Config precedence** | flags › env (`RUVIEW_URL`, `RUVIEW_TOKEN`) › `~/.ruview/config.toml` › built-in defaults. |
| **Secrets** | Never via flags (shell history). WiFi/OTA passwords via prompt (no echo), stdin, or file. |
| **Confirmations** | Destructive ops (erase NVS, delete user/tenant, factory OTA) prompt unless `--yes`; only prompt when stdin is a TTY; `--no-input` for CI. |
| **Progress** | Spinners/progress bars for flash/OTA/nexmon (long ops); print first output <100 ms; stream logs on error. |
| **Exit codes** | 0 ok; categorized non-zero (2 usage, 3 not-found, 4 auth, 5 hardware, 6 network) so scripts can branch. |
| **Discoverability** | `--help` with examples first; `ruview help <topic>`; "next command" hints; link to docs. |
| **Robustness** | Validate early; network timeouts; idempotent where possible; resume long ops; clean Ctrl-C. |
| **Extensibility** | Unknown verb → look up `wifi-densepose-<verb>` on PATH (gh/git plugin pattern). |

---

## 2. Architecture — three backends behind one CLI

The management surface is split across the system; the CLI unifies it:

```
                    ┌───────────────────────── wifi-densepose (CLI) ─────────────────────────┐
                    │                                                                          │
  A) REST client ───┼──►  sensing-server  http://127.0.0.1:4000  (nodes, sensing,             │
                    │       calibration, models, recording, training, pose, health)           │
  B) process mgr ───┼──►  spawn/stop the bundled sensing-server  (server lifecycle)            │
  C) direct/local ──┼──►  esptool / native NVS gen / netsh / serial / SSH                      │
                    │       (flash, provision, discover, wifi-scan, OTA, Pi)                    │
  D) local admin ───┼──►  wave.db (SQLite) via shared auth crate                                │
                    │       (login, users, roles, tenants, license, settings)                  │
                    └──────────────────────────────────────────────────────────────────────────┘
```

- **A/B** are thin and land first (highest value, lowest risk).
- **C** reuses the exact logic the desktop `commands/{flash,provision,discovery,ota,pi_node}.rs` already use — **extract that logic into a shared `wifi-densepose-manage` crate** so desktop + CLI share one implementation (no drift). This is the main refactor.
- **D** (auth/users/tenants/license) lives in the desktop's `wave.db`. The CLI links the shared `auth` code and opens the same DB directly (same machine). **Open decision:** direct-DB vs a new admin REST surface (see §7).

---

## 3. Complete backend endpoint inventory

### 3a. Sensing-server REST/WS (≈50 routes) — reached by CLI backend **A**

| Group | Routes | CLI surface |
|-------|--------|-------------|
| **Health** | `/health`, `/health/{ready,live,version,metrics}`, `/api/v1/{status,info,metrics}` | `ruview server status`, `ruview doctor` |
| **Nodes** | `GET /api/v1/nodes` | `ruview node list/get/watch` |
| **Sensing** | `GET /api/v1/sensing/latest`, `/vital-signs`, `/edge-vitals`, `/wasm-events` | `ruview sensing latest/vitals/watch` |
| **Pose** | `/api/v1/pose/current`, `/pose/stats`, `/pose/zones/summary` | `ruview pose current/stats/zones` |
| **Streams** | `WS /ws/sensing`, `/api/v1/stream/pose`, `/stream/status` | `ruview sensing watch`, `ruview pose watch` |
| **Calibration** | `POST /calibration/start\|stop`, `GET /status` | `ruview calibrate start/stop/status` |
| **Recording** | `GET /recording/list`, `POST /start\|stop`, `DELETE /{id}` | `ruview recording list/start/stop/rm` |
| **Training** | `POST /train/start\|stop`, `GET /status` | `ruview train start/stop/status` |
| **Adaptive** | `POST /adaptive/train\|unload`, `GET /status` | `ruview adaptive train/unload/status` |
| **Models** | `GET /models`, `/models/active`, `POST /models/load\|unload`, `DELETE /{id}`, `/model/info\|layers\|segments`, `/model/sona/*`, `/models/lora/*` | `ruview model list/load/unload/info/sona/lora` |
| **Ranging (FTM)** | `POST /ranging/run\|apply`, `GET /status` | `ruview ranging run/apply/status` |
| **Positions** | `GET/PUT /config/node-positions` | `ruview node place/positions` |

### 3b. Desktop Tauri commands (92) — reached by CLI backends **B/C/D**

| Module | Commands | CLI surface | Backend |
|--------|----------|-------------|---------|
| **server** | start, stop, restart, status, logs | `ruview server *` | B |
| **discovery** | discover_nodes, list_serial_ports, host_network_info, scan_wifi_networks, wifi_saved_password, configure_esp32_wifi | `ruview node discover`, `ruview serial list`, `ruview wifi scan/host` | C |
| **flash** | flash_firmware, flash_firmware_bundle, flash_progress, verify_firmware, check_espflash, supported_chips, fetch_firmware_release | `ruview esp32 flash/verify/chips`, `ruview firmware fetch` | C |
| **provision** | provision_esp32_nvs, esp32_serial_check, read_nvs, erase_nvs, validate_config, provision_node, generate_mesh_configs | `ruview esp32 provision/verify/read-nvs/erase-nvs`, `ruview mesh gen` | C |
| **ota** | ota_update, batch_ota_update, check_ota_endpoint | `ruview ota update/batch/check` | C |
| **pi_node** | probe, check_prereqs, install_nexmon, build_agent, deploy_binary, push_config, install_service, service, csi_health | `ruview pi probe/prereqs/nexmon/deploy/service/health` | C |
| **wasm** | wasm_list, wasm_upload, wasm_control, wasm_info, wasm_stats, check_wasm_support | `ruview wasm list/upload/start/stop/info` | C |
| **models** | list_bundled_models | `ruview model bundled` | C |
| **auth** | login, logout, refresh_token, get_current_user, change_password | `ruview login/logout/whoami/passwd` | D |
| **users** | list, get, create, update, delete, assign_user_role | `ruview user *` | D |
| **roles** | list, get, create, delete, set_role_permissions, list_modules, list_tenant_modules | `ruview role *` | D |
| **license/tenants** | activate_license, license_status, list/create/delete_tenant, assign_tenant_modules | `ruview license *`, `ruview tenant *` | D |
| **plan** | get_plan_tier | `ruview plan` | D |
| **settings** | get_settings, save_settings | `ruview config get/set` | D |
| **deployment** | register, get/set_info, list, aggregate | `ruview deployment *` | D/REST |
| **cloud** | get_cloud_config, set_consent, upload_sensing_session | `ruview cloud config/consent/upload` | D |
| **enterprise** | get/save settings, whapi_get_qr/send_test/send_alert | `ruview alerts qr/test/send`, `ruview config enterprise` | D |
| **telemetry** | get/set_langfuse_config | `ruview config telemetry` | D |
| **frappe** | get/set_frappe_config | `ruview config frappe` | D |
| **analytics** | run_insight_pipeline, get_session_insight, get_analytics_trends, get_risk_distribution | `ruview insight run/get/trends/risk` | D/REST |
| **security_keys** | (SSH key admin) | `ruview sshkey *` | D |

---

## 4. CLI command taxonomy (noun-verb)

```
ruview
├── login / logout / whoami / passwd          # auth (D)
├── server   start|stop|restart|status|logs   # B
├── doctor                                     # env + connectivity self-check
├── node     list|get|watch|place|positions   # A
├── serial   list                             # C
├── wifi     scan|host                         # C
├── esp32    flash|provision|verify|read-nvs|erase-nvs|serial-check|chips   # C
├── firmware fetch|releases                    # C
├── mesh     gen                               # C
├── ota      update|batch|check                # C
├── pi       probe|prereqs|nexmon|deploy|service|health   # C
├── sensing  latest|vitals|watch               # A
├── pose     current|stats|zones|watch         # A
├── calibrate start|stop|status                # A
├── recording list|start|stop|rm               # A
├── model    list|bundled|load|unload|info|sona|lora   # A/C
├── train    start|stop|status                 # A
├── adaptive train|unload|status               # A
├── ranging  run|apply|status                  # A
├── wasm     list|upload|start|stop|info       # C
├── user     list|get|create|update|delete|role   # D
├── role     list|get|create|delete|perms      # D
├── tenant   list|create|delete|modules        # D
├── license  status|activate                   # D
├── plan                                        # D
├── deployment list|register|info|aggregate    # D
├── cloud    config|consent|upload             # D
├── insight  run|get|trends|risk               # D
├── alerts   qr|test|send                       # D (whapi)
├── sshkey   list|add|rm                        # D
├── config   get|set|list [--section]           # D
└── mat ...                                      # existing
```

Global flags: `-o/--output table|json|yaml`, `--url`, `--token`, `-q/--quiet`,
`-v/--verbose`, `--no-color`, `-y/--yes`, `--no-input`, `--dry-run`.

---

## 5. User flows (per use case)

**Onboard a first ESP32 node (end-to-end, headless):**
```
ruview doctor                              # server up? esptool present? PATH ok?
ruview serial list                         # find the board (COM7)
ruview wifi scan                           # pick 2.4GHz SSID
ruview esp32 flash --port COM7 --chip esp32s3            # progress bar
ruview esp32 provision --port COM7 --ssid "Net" --ask-password --node-id 1
ruview esp32 serial-check --port COM7      # "joined WiFi? got IP?"
ruview node watch --id 1                   # wait until it streams
```

**Set up a Raspberry Pi (nexmon):**
```
ruview pi probe --host pi@192.168.1.50
ruview pi nexmon --host pi@192.168.1.50 --yes   # streams install log, waits reboot
ruview pi deploy --host pi@192.168.1.50
ruview pi health --host pi@192.168.1.50
```

**Live monitoring / scripting:**
```
ruview node list -o json | jq '.[] | select(.status=="active")'
ruview sensing watch                        # live presence/persons/vitals
ruview pose zones -o json
```

**Calibrate an empty room:**
```
ruview calibrate start --duration 30
ruview calibrate status
```

**Fleet OTA:**
```
ruview firmware fetch --tag v0.8.3-esp32
ruview ota batch --nodes 192.168.1.11,192.168.1.12 --file app.bin --concurrency 2
```

**User / tenant admin:**
```
ruview login admin@wave.io
ruview user create --email a@b.io --role operator
ruview role perms operator --grant sensing,pose-3d
ruview tenant create --name "Site A" --modules sensing,firmware
```

**Server lifecycle & diagnostics:**
```
ruview server start                         # spawns bundled sensing-server
ruview server status -o json
ruview server logs --tail 100
ruview doctor
```

**Models / on-device inference (ties to the inference plan):**
```
ruview model bundled                        # list shipped weights
ruview model list                           # server-loaded models
ruview node get --id 1 -o json | jq .inference   # per-node model output (future)
```

---

## 6. Cross-cutting

- **Output engine:** one renderer with `table` (comfy-table/tabled), `json`, `yaml`; auto-select by `isatty(stdout)`.
- **Config:** `~/.ruview/config.toml` (url, token, default output) + `RUVIEW_*` env; `ruview config` reads/writes it.
- **Auth/token:** `ruview login` stores a token (keyring/`~/.ruview`); commands attach it; `--token`/env override.
- **Server discovery:** default `http://127.0.0.1:4000`; `--url` / `RUVIEW_URL` override; read from settings if present.
- **Bundling:** ship `wifi-densepose(.exe)` as a Tauri `externalBin` sidecar (like `sensing-server`) + stage it per-platform in CI; optionally add an installer step to put it on PATH.

---

## 7. Open decisions (need your call)

1. **Auth/user/tenant management transport (backend D):** open `wave.db` directly (simplest, same-machine) **vs** add an authenticated admin REST surface to the server (works remotely, cleaner boundary, more work). *Recommend: direct-DB now, admin-API later if remote mgmt is needed.*
2. **Binary name / alias:** keep `wifi-densepose`, add short `ruview` alias? *Recommend: yes, `ruview` is the ergonomic name.*
3. **Shared-logic refactor scope:** extract desktop `commands/{flash,provision,discovery,ota,pi_node}` into a `wifi-densepose-manage` crate now (clean, no drift) vs the CLI re-implementing thin wrappers first. *Recommend: extract — one implementation.*

---

## 8. Phased implementation

| Phase | Scope | Value / risk |
|-------|-------|--------------|
| **P1 — Foundation** | CLI skeleton: global flags, output engine (table/json/yaml + TTY detect), config/env, exit codes, `doctor`. | Low risk; unlocks everything. |
| **P2 — Read/monitor (REST)** | `server status`, `node list/get/watch`, `sensing latest/watch`, `pose`, `calibrate`, `recording`, `model list`, `health`. | High value, thin HTTP; scripting immediately. |
| **P3 — Server lifecycle** | `server start/stop/restart/logs` (spawn bundled sensing-server). | Makes it a real control plane. |
| **P4 — Hardware (shared crate)** | Extract `wifi-densepose-manage`; `serial`, `wifi scan`, `esp32 flash/provision/verify`, `firmware`, `ota`, `mesh`. | The big one; provisioning without the GUI. |
| **P5 — Pi** | `pi probe/prereqs/nexmon/deploy/service/health` (SSH streaming). | Full Pi bring-up headless. |
| **P6 — Admin (D)** | `login`, `user`, `role`, `tenant`, `license`, `plan`, `config`, `settings`. | Needs the §7.1 decision. |
| **P7 — Enterprise/cloud** | `deployment`, `cloud`, `insight`, `alerts`, `sshkey`, `telemetry/frappe config`, `wasm`. | Rounds out "everything." |
| **P8 — Polish** | Shell completions (bash/zsh/fish/pwsh), `man`/docs, plugin lookup, packaging on PATH, `--watch` niceties. | DX shine. |

Each phase is independently shippable and testable (`assert_cmd` integration tests
+ a mock server for the REST-backed commands).

---

## Sources
- [Command Line Interface Guidelines — clig.dev](https://clig.dev/)
- [cli-guidelines/cli-guidelines (GitHub)](https://github.com/cli-guidelines/cli-guidelines)
- [UX patterns for CLI tools — Lucas F. Costa](https://www.lucasfcosta.com/blog/ux-patterns-cli-tools)
- [14 tips to make amazing CLI applications — dev.to](https://dev.to/wesen/14-great-tips-to-make-amazing-cli-applications-3gp3)
- [Elevate developer experiences with CLI design guidelines — Thoughtworks](https://www.thoughtworks.com/insights/blog/engineering-effectiveness/elevate-developer-experiences-cli-design-guidelines)
