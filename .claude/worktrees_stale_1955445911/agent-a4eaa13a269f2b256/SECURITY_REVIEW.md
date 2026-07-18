# Security Review — wifi-densepose / RuView (2026-06-20)

**Scope**: Entire folder, git-independent, folder-wide (not a diff/branch review).  
**Agents**: 5 parallel sub-agents. Git repo: yes.  
**Partition**:
1. Python — `v1/`, `wifi_densepose/`, `data/`
2. Rust workspace — `rust-port/wifi-densepose-rs/`
3. Frontend — `ui/`, `assets/`, `examples/`
4. Firmware/edge — `firmware/`, `nexmon_csi/`, `ruview_pi_files/`, `deploy/`, `vendor/`
5. Infra/root — `docker/`, `.github/workflows/`, `scripts/`, `monitoring/`, `logging/`, all loose root files

---

## Summary

| Category | Critical | High | Medium | Low | Needs-human |
|----------|----------|------|--------|-----|-------------|
| Confirmed | 2 | 8 | 8 | 6 | — |
| Needs-human | — | — | — | — | 5 |
| False-positive | — | — | — | — | 8 |

**Top priority (immediate):**
1. `tmp-ssh-askpass.cmd` — SSH passphrase committed to git (C-1, Critical)
2. ESP32 OTA firmware update server accepts any firmware, no auth (C-2, Critical)
3. Rust REST endpoints have no authentication on destructive operations (H-1, High)

---

## Confirmed Findings

### CRITICAL

---

#### C-1: SSH Passphrase Committed to Repository
- **Severity**: Critical
- **File**: `tmp-ssh-askpass.cmd:2`
- **Evidence**: File contains `echo The$1000`. It is a Windows SSH askpass helper (runs as `SSH_ASKPASS` helper, outputs passphrase to stdout). File is git-tracked and appears in commit `4434a580` ("chore: import comprehensive project dependencies, build tools, and external utilities").
- **Why exploitable**: Any clone of this repository — including forks, CI runners, and past collaborators — can read this passphrase. If any SSH key on this machine or a connected system uses `The$1000` as its passphrase, that key's passphrase protection is nullified.
- **Verification**: `git ls-files tmp-ssh-askpass.cmd` → tracked. `git log --oneline -- tmp-ssh-askpass.cmd` → `4434a580`. File content directly read and matches.
- **Fix**:
  1. Rotate any SSH key that uses `The$1000` as its passphrase immediately.
  2. Remove the file from git history: `git filter-repo --path tmp-ssh-askpass.cmd --invert-paths`.
  3. Add `tmp-ssh-askpass*` to `.gitignore`.

---

#### C-2: ESP32 OTA Firmware Update: PSK Auth Permanently Bypassed
- **Severity**: Critical
- **File**: `firmware/esp32-csi-node/main/main.c:207`, `firmware/esp32-csi-node/main/ota_update.c:37,44-48,244-260,263-266`
- **Evidence**: `main.c:207` calls `ota_update_init_ex(&ota_server)`. That function (`ota_update.c:263-266`) starts the HTTP OTA server without loading the PSK from NVS. The PSK-loading code is in `ota_update_init()` (`ota_update.c:244-260`) — verified by `grep -rn ota_update_init` across all `.c` files: `ota_update_init()` is defined but never called. Consequently `s_ota_psk[0]` remains `'\0'` (its zero-initialized value at line 37). `ota_check_auth()` (`ota_update.c:46-48`) returns `true` unconditionally when `s_ota_psk[0] == '\0'`, admitting any OTA upload.
- **Why exploitable**: Any attacker with network access to port 8032 (the OTA HTTP server) can upload an arbitrary firmware binary and trigger `esp_restart()` to boot it. This achieves complete device takeover (remote code execution). No valid credential is required.
- **Verification**: Direct code trace: `main.c:207` → `ota_update_init_ex()` → `ota_start_server()` (no PSK load). `ota_check_auth()` line 46: `if (s_ota_psk[0] == '\0') return true;`. NVS encryption is also disabled: `sdkconfig.defaults:23` has `# CONFIG_NVS_ENCRYPTION is not set`.
- **Fix**: Change `main.c:207` from `ota_update_init_ex(&ota_server)` to `ota_update_init()` (which loads the PSK before starting the server). Verify with integration test that upload is rejected without PSK.

---

### HIGH

---

#### H-1: Rust Sensing Server — No Authentication on Destructive REST Endpoints
- **Severity**: High
- **File**: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs:2746-2778` (delete_model), `main.rs:3015-3042` (delete_recording), `main.rs:2717-2743` (load_model), `main.rs:4804-4806` (calibration)
- **Evidence**: Handlers `delete_model`, `delete_recording`, `load_model`, `calibration_start` have no auth parameter in their function signatures. Router setup (`main.rs:4764-4832`) adds no auth middleware layer.
- **Why exploitable**: Any network client can call `DELETE /api/v1/models/{id}` to erase trained models, `POST /api/v1/models/load` to swap the active model, or `POST /api/v1/calibration/start` to wipe calibration state. If the server binds to `0.0.0.0` (via `SENSING_BIND_ADDR` env var), exposure is network-wide.
- **Verification**: Code trace: handler signatures lack auth deps; router has no middleware auth layer.
- **Fix**: Add authentication middleware: `Router::new().layer(axum::middleware::from_fn(auth_middleware))` where `auth_middleware` validates a Bearer token or API key. Default bind address is `127.0.0.1` (good); log a warning if overridden to `0.0.0.0`.

---

#### H-2: Rust Sensing Server — Path Traversal (Partial Mitigation) + No Auth on File Deletion
- **Severity**: High
- **File**: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs:2750-2758`, `main.rs:3020-3026`
- **Evidence**: `safe_id != id` check prevents basic `../` traversal but no auth gate precedes it. Any unauthenticated client can delete any `.rvf` file in the models directory. Path is not validated against the base directory after join.
- **Why exploitable**: No auth means any network client can delete arbitrary model files. Double URL-encoding bypass (`%252F` → `%2F` → `/`) could potentially defeat the filename-extraction check.
- **Verification**: Code trace; confirmed no auth before deletion; missing `path.starts_with(models_dir)` after `join()`.
- **Fix**: Add auth (see H-1). Add post-join path containment check: `if !path.starts_with(effective_models_dir()) { return error; }`. Restrict `safe_id` to alphanumeric + hyphen/underscore via regex.

---

#### H-3: Python API — Token Blacklist Clears All Entries Every Hour (Logout Bypass)
- **Severity**: High
- **File**: `v1/src/api/middleware/auth.py:231-257`
- **Evidence**: `TokenBlacklist` class at line 255 calls `self._blacklisted_tokens.clear()` in the cleanup task without checking token expiry. All revoked tokens re-become valid after one hour, regardless of their JWT expiration time.
- **Why exploitable**: An attacker who captures a victim's JWT can wait up to 1 hour after the victim logs out and then reuse the token. Logout protection fails after every cleanup cycle.
- **Verification**: Direct code read at line 255.
- **Fix**: Store `{token_hash: exp_timestamp}` pairs. Only evict entries where `exp_timestamp < now`. Use Redis for persistence across restarts.

---

#### H-4: Python API — Unauthenticated Metrics Endpoint
- **Severity**: High
- **File**: `v1/src/api/routers/stream.py:507-523`
- **Evidence**: `get_streaming_metrics()` has no `Depends(require_auth)` parameter. Neighbouring endpoints (`/stream/start:362`, `/stream/stop:392`, `/stream/clients:416`) all require auth. The metrics endpoint is inconsistently unguarded.
- **Why exploitable**: Unauthenticated callers can retrieve streaming performance metrics, connection counts, and system state.
- **Verification**: Code trace comparing endpoint signatures.
- **Fix**: Add `current_user: Dict = Depends(require_auth)` to `get_streaming_metrics()`.

---

#### H-5: GitHub Actions Pinned to Mutable Branches (`@master`/`@main`)
- **Severity**: High
- **File**: `.github/workflows/ci.yml:258`, `.github/workflows/security-scan.yml:166,224,241,250,280`
- **Evidence**: Direct file read confirms:
  - `ci.yml:258`: `uses: aquasecurity/trivy-action@master`
  - `security-scan.yml:166`: `aquasecurity/trivy-action@master`
  - `security-scan.yml:224`: `bridgecrewio/checkov-action@master`
  - `security-scan.yml:241`: `tenable/terrascan-action@main`
  - `security-scan.yml:280`: `trufflesecurity/trufflehog@main`
- **Why exploitable**: A compromised or malicious push to the action repository's default branch silently changes behavior on next workflow run. These are security-critical actions (secret scanning, vulnerability scanning) — a compromised version could exfiltrate secrets or suppress findings.
- **Verification**: Directly confirmed in committed files.
- **Fix**: Pin to a specific commit SHA or semantic version tag. Enable Dependabot for GitHub Actions to automate updates with PR review.

---

#### H-6: TLS Certificate Verification Disabled in Prometheus (Kubernetes Scrape)
- **Severity**: High
- **File**: `monitoring/prometheus-config.yml:42,56,242`
- **Evidence**: `grep -n insecure_skip_verify monitoring/prometheus-config.yml` → three confirmed matches.
- **Why exploitable**: Disables certificate validation for Kubernetes API server, nodes, and cAdvisor scrapes. An attacker on the cluster network can MitM Prometheus scrapes, inject false metrics, or intercept Kubernetes API responses.
- **Verification**: Direct grep on committed file.
- **Fix**: Remove `insecure_skip_verify: true`. Use the in-cluster CA already referenced: `ca_file: /var/run/secrets/kubernetes.io/serviceaccount/ca.crt`.

---

#### H-7: Python API — Any Authenticated User Can Disconnect Any WebSocket Client
- **Severity**: High
- **File**: `v1/src/api/routers/stream.py:436-465`
- **Evidence**: `DELETE /api/v1/stream/clients/{client_id}` calls `connection_manager.disconnect(client_id)` without validating that `current_user` owns or has permission over that `client_id`. No role check.
- **Why exploitable**: An authenticated attacker can disconnect other users' live streaming sessions by iterating or guessing client IDs.
- **Verification**: Code trace; no ownership/role check before disconnect.
- **Fix**: Track client ownership in `connection_manager`. Validate `current_user['id']` matches the client's owner, or check admin role.

---

#### H-8: Python API — CORS Wildcard with Credentials Enabled
- **Severity**: High
- **File**: `v1/src/config/settings.py:296-311`, `v1/src/api/main.py:159-163`
- **Evidence**: Development mode configures `allow_origins: ["*"]` with `allow_credentials: True` and `allow_methods: ["*"]` (settings.py:298-303).
- **Why exploitable**: The `allow_credentials=True` + `allow_origins: ["*"]` combination violates the CORS spec; spec-compliant browsers reject it, limiting blast radius. However, the misconfiguration is active and enables CSRF attacks against authenticated users in non-compliant clients and would be an open door if `allow_origins` were tightened incorrectly.
- **Verification**: Direct code read at settings.py:298-303.
- **Fix**: In production, set `allow_origins` to an explicit list of allowed origins. Never combine `allow_origins: ["*"]` with `allow_credentials: True`. Flip `ENABLE_AUTHENTICATION=false` to `true` in production.

---

### MEDIUM

---

#### M-1: Rust Sensing Server — UDP Unbounded Vector Allocation (DoS)
- **Severity**: Medium
- **File**: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/protocol/esp32_legacy.rs:64-77`
- **Evidence**: Parser reads `n_antennas` and `n_sub_u16` from untrusted UDP packet header and computes `n_pairs = n_antennas as usize * n_sub_u16 as usize`. Validates `buf.len() >= expected_len` but does not bound `n_pairs`. Then calls `Vec::with_capacity(n_pairs)`.
- **Why exploitable**: Attacker sends UDP packets with `n_antennas=255, n_sub_u16=65535` → `n_pairs=16.7M` → `Vec::with_capacity(16.7M)` per packet. UDP receiver runs in a hot loop. Rapid flooding exhausts heap → DoS.
- **Verification**: Code trace confirmed; no max check before allocation.
- **Fix**: `const MAX_PAIRS: usize = 256 * 4; if n_pairs > MAX_PAIRS { return None; }`

---

#### M-2: Rust Sensing Server — WASM Packets Broadcast Without Validation
- **Severity**: Medium
- **File**: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs:3675-3691`
- **Evidence**: WASM output packets (magic `0xC511_0006`) accepted and broadcast via WebSocket with no whitelist of `module_id` or `event_type`.
- **Why exploitable**: Attacker spoofs WASM packets with arbitrary `module_id` and crafted `events` fields → downstream UI or WASM consumers may misinterpret, enable unintended modes, or expose parsing bugs.
- **Verification**: Code trace; no validation before `s.tx.send(json)`.
- **Fix**: Maintain `ALLOWED_MODULES: &[u8]` whitelist; reject unknown module IDs with a warning.

---

#### M-3: Rust Sensing Server — WebSocket Streams Sensitive Data Without Auth
- **Severity**: Medium
- **File**: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs:1829-1834,1870-1875`
- **Evidence**: `ws_sensing_handler` and `ws_pose_handler` accept connections without origin or token validation. Anyone who can reach the port subscribes to live breathing rate, heart rate, motion, and pose keypoints.
- **Verification**: Handler signatures confirmed; no auth parameter or origin check.
- **Fix**: Validate `Origin` header against an allowlist; require Bearer token in first WebSocket message; reject otherwise.

---

#### M-4: Rust Sensing Server — `MODELS_DIR` Env Var Not Validated
- **Severity**: Medium
- **File**: `rust-port/wifi-densepose-rs/crates/wifi-densepose-sensing-server/src/main.rs:2805-2809`
- **Evidence**: `effective_models_dir()` reads `MODELS_DIR` env var without path validation.
- **Why exploitable**: In a container escape or CI/CD compromise, attacker sets `MODELS_DIR=/etc` and calls `GET /api/v1/models` to list `/etc` files, or loads arbitrary files as models.
- **Verification**: Code confirmed; no canonicalization or bounds check.
- **Fix**: Validate on startup that `MODELS_DIR` is a relative path or within a known safe prefix; reject absolute paths pointing outside the app directory.

---

#### M-5: Python API — WebSocket Token in URL Query Parameter
- **Severity**: Medium
- **File**: `v1/src/api/middleware/auth.py:164-181`
- **Evidence**: Auth middleware extracts tokens from `request.query_params.get("token")` at line 172, allowing `ws://host/stream?token=eyJ...`. Tokens in URLs appear in server access logs, proxy logs, and browser history.
- **Verification**: Direct code read.
- **Fix**: Remove query-param extraction. Require auth token in WebSocket first message only (already partially implemented at route level).

---

#### M-6: Python API — In-Memory Rate Limiting (Ineffective Multi-Instance)
- **Severity**: Medium
- **File**: `v1/src/api/middleware/rate_limit.py:27-29`
- **Evidence**: `defaultdict(lambda: deque())` used for rate limit storage. Comment acknowledges Redis needed for production.
- **Why exploitable**: Rate limits reset on restart and are not shared across instances; brute-force and DoS protection is bypassed in multi-instance deployments.
- **Verification**: Code confirmed.
- **Fix**: Implement Redis-backed rate limiting; document as required for production.

---

#### M-7: Python API — Database Credentials in Connection URL Strings (May Log)
- **Severity**: Medium
- **File**: `v1/src/database/connection.py:97-104`
- **Evidence**: `f"postgresql://{user}:{password}@{host}:{port}/{name}"` at lines 98-104. SQLAlchemy `echo=True` (lines 116, 174) logs full connection info.
- **Why exploitable**: DB credentials appear in application logs if connection fails or if `echo=True` is active.
- **Verification**: Direct code read.
- **Fix**: Use SQLAlchemy `URL.create()` with `hide_password()` for any logging. Set `echo=False` in production; gate it behind `DEBUG` env var.

---

#### M-8: GitHub Actions — Insecure kubeconfig Handling in CD Pipeline
- **Severity**: Medium
- **File**: `.github/workflows/cd.yml:97,143`
- **Evidence**: `echo "${{ secrets.KUBE_CONFIG_DATA_STAGING }}" | base64 -d > kubeconfig` — written to workspace without explicit cleanup.
- **Why exploitable**: kubeconfig in working directory without `chmod 600` or cleanup may persist in logs or CI caches.
- **Verification**: Direct file read.
- **Fix**: `KUBECONFIG=$(mktemp); chmod 600 "$KUBECONFIG"; trap "rm -f $KUBECONFIG" EXIT; echo "..." | base64 -d > "$KUBECONFIG"`.

---

### LOW

---

#### L-1: ESP32 Credentials Stored in Plaintext NVS (Physical Access Risk)
- **Severity**: Low
- **File**: `firmware/esp32-csi-node/main/nvs_config.c:27-32,310-311`, `sdkconfig.defaults:23`
- **Evidence**: WiFi SSID, password, and Seed bearer token stored in NVS without encryption. `CONFIG_NVS_ENCRYPTION` is explicitly unset in `sdkconfig.defaults`.
- **Why exploitable**: Physical attacker can read flash with `esptool.py` and extract plaintext WiFi credentials and seed token.
- **Fix**: Enable `CONFIG_NVS_ENCRYPTION=y` in sdkconfig; provision encryption key from eFuse or secure element.

---

#### L-2: ESP32 OTA Status Endpoint Exposes Firmware Version (Unauthenticated)
- **Severity**: Low
- **File**: `firmware/esp32-csi-node/main/ota_update.c:78-97,220-226`
- **Evidence**: `GET /ota/status` returns firmware version, build date, partition info without calling `ota_check_auth()`.
- **Fix**: Apply `ota_check_auth()` to status handler, or disable in production builds.

---

#### L-3: Nexmon CSI Config File Written Without Permission Restriction
- **Severity**: Low
- **File**: `ruview_pi_files/nexmon_startup.sh:69`, `ruview_pi_files/nexmon_setup_auto.sh:341-343`
- **Evidence**: Config written to `~/.config/nexmon/csi_config` without `chmod 600`.
- **Fix**: Add `chmod 600 "$CONFIG_SAVE_FILE"` after writing.

---

#### L-4: Python API — CSP Allows `unsafe-inline`
- **Severity**: Low
- **File**: `v1/src/api/middleware/auth.py:273-279`
- **Evidence**: `script-src 'self' 'unsafe-inline'` and `style-src 'self' 'unsafe-inline'` — `unsafe-inline` negates XSS protection of CSP.
- **Fix**: Remove `'unsafe-inline'`; use nonce or hash-based CSP for any inline content.

---

#### L-5: Frontend — Missing postMessage Origin Validation in WebViews
- **Severity**: Low (bounded scope — mobile WebView only)
- **File**: `ui/mobile/src/assets/webview/gaussian-splats.html:461`, `ui/mobile/src/assets/webview/mat-dashboard.html:464`
- **Evidence**: `window.addEventListener('message', ...)` handlers do not check `event.origin`.
- **Fix**: `if (!event.origin.startsWith(window.location.origin)) return;`

---

#### L-6: Both Dockerfiles Run as Root (No `USER` Directive)
- **Severity**: Low
- **File**: `docker/Dockerfile.rust`, `docker/Dockerfile.python`
- **Evidence**: Neither Dockerfile contains a `USER` directive in the final stage; confirmed by direct grep.
- **Why exploitable**: Container compromise gives attacker root within the container, easing container escape.
- **Fix**: Add `RUN useradd -m -u 1001 app && USER app` before the entrypoint in both Dockerfiles.

---

## Needs-Human Review

| # | Item | Why Human Needed |
|---|------|-----------------|
| NH-1 | `ui/components/ModelPanel.js:160` — `frames_processed` in innerHTML | Risk depends on whether `/api/v1/models/active` accepts externally-controlled model metadata. If yes: confirmed XSS vector. |
| NH-2 | `ui/mobile/package.json` — JS dependency CVE check | npm audit blocked; run `npm audit` in `ui/mobile/` and remediate High/Critical findings. |
| NH-3 | `vendor/` submodules not checked out (empty) | Vendored Rust crates `ruvector`, `midstream`, `sublinear-time-solver` cannot be audited without `git submodule update --init --recursive`. Run `cargo audit` after initialization. |
| NH-4 | `v1/src/tasks/backup.py:184-196` — config backup path pattern | Currently hardcoded (safe). Future dynamic extension without `Path.resolve()` + whitelist would introduce path traversal. Flag for code review before any future change. |
| NH-5 | `.github/workflows/security-scan.yml` — external secrets `SEMGREP_APP_TOKEN`, `SNYK_TOKEN`, `GITLEAKS_LICENSE` | Confirm these secrets are scoped correctly in the GitHub repo settings and not over-permissioned. |

---

## Dismissed (False Positives)

| Item | Why Dismissed |
|------|---------------|
| `example.env` — `SECRET_KEY=your-secret-key-here` | Template placeholder, not a live credential |
| `example.env` — `CORS_ORIGINS=*`, `ENABLE_AUTHENTICATION=false` | Development defaults with explicit production instructions; not a deployed config |
| `sensing-server.out.log` / `.err.log` — committed log files | Contain only startup INFO messages; no credentials or sensitive data confirmed by reading |
| GitHub Actions `actions/checkout@v4`, `setup-python@v5` | Version-tagged (not SHA), but official GitHub-maintained — acceptable risk vs. `@master` on third-party security tools |
| `plans/` directory | No credentials; only API specification documentation |
| `excalidraw.log` | Design tool log; no sensitive content confirmed |
| `.mcp.json` | MCP server configuration; no secrets |
| `.swarm/state.json` | Swarm coordination state; no credentials |

---

*Report only — no fixes applied. Remediation is a separate, user-approved step.*  
*Verification method for each finding: `confirmed` = code-traced to reachable sink OR file directly read and value confirmed live; `needs-human` = cannot determine without runtime context or external tool access.*
