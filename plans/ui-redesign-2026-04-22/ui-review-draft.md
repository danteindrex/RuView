# UI Review Draft (Hardline)

Date: 2026-04-22  
Scope evaluated:
- `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/*`
- `ui/*` legacy web UI surface

Verdict: the current UI is not production-grade. It mixes exploratory/demo behavior with operational controls, has weak interaction consistency, and relies on ad-hoc styling instead of a component system.

## Priority Findings

### Severity 4 (Blocker)

1. Demo/simulated modes are embedded in primary product flows.
- Evidence:
  - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/types.ts:173` (`"simulate"` in data source type)
  - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/hooks/useServer.ts:12` (default source is `"simulate"`)
  - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/pages/Sensing.tsx:306`
  - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/pages/Sensing.tsx:560`
  - Legacy `ui/` contains broad demo/mock infrastructure (for example `ui/app.js`, `ui/components/LiveDemoTab.js`, `ui/services/sensing.service.js`, `ui/utils/mock-server.js`)
- Why this fails production:
  - Violates environment clarity and trust. Operators cannot reliably distinguish real telemetry vs synthetic behavior at a glance.

2. Mesh view is not a trustworthy operational view.
- Evidence:
  - Randomized topology/layout forces:
    - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/pages/MeshView.tsx:82`
    - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/pages/MeshView.tsx:83`
    - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/pages/MeshView.tsx:99`
    - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/pages/MeshView.tsx:106`
    - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/pages/MeshView.tsx:115`
  - Fabricated-looking telemetry labels:
    - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/pages/MeshView.tsx:577` (`Drift`)
    - `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/pages/MeshView.tsx:581` (`Cycle`)
- Why this fails production:
  - Visualization confidence is compromised. When geometry is random, operators cannot trust state.

### Severity 3 (Major)

3. No design-system discipline (high style entropy, weak consistency).
- Evidence:
  - Inline style count in desktop UI source: `479` occurrences of `style={{...}}`
  - Example shell starts in `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/src/App.tsx`
  - No shadcn dependencies in `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui/package.json`
- Why this fails production:
  - Hard to maintain, hard to theme, hard to test, and impossible to scale without regressions.

4. UI architecture is too monolithic in key pages.
- Evidence:
  - `EdgeModules.tsx` ~1729 lines
  - `NetworkDiscovery.tsx` ~1210 lines
  - `Sensing.tsx` ~682 lines
  - `MeshView.tsx` ~639 lines
- Why this fails production:
  - Large page files blend data fetching, state, and presentation. This slows change velocity and increases defect risk.

5. Non-professional iconography/text artifacts in operational UI.
- Evidence:
  - Unicode icon navigation in `App.tsx:31-39`
  - Symbol/emoji usage in `NetworkDiscovery.tsx:339`, `:370`, `:392`, `:439`
  - Star glyph ratings in `EdgeModules.tsx:1052-1054`
- Why this fails production:
  - Inconsistent tone and low information density for enterprise operations.

### Severity 2 (Important)

6. “Advanced” controls are spread across pages without a single control surface.
- Evidence:
  - Flash, OTA, server/source, and module controls are split and repeated with different patterns.
- Why this fails production:
  - Operators need one grouped “Advanced Settings” surface for risk-prone actions and operational defaults.

7. Legacy and desktop surfaces are drifting in behavior model.
- Evidence:
  - Desktop Tauri UI and legacy `ui/` both include sensing/view flows but use different assumptions and fallback semantics.
- Why this fails production:
  - Fragmented UX model leads to inconsistent behavior and harder support/debugging.

## Functionality That Must Be Preserved

Tauri command surface currently wired in:
- `discover_nodes`, `list_serial_ports`, `configure_esp32_wifi`
- `flash_firmware`, `flash_progress`, `verify_firmware`, `check_espflash`, `supported_chips`
- `ota_update`, `batch_ota_update`, `check_ota_endpoint`
- `wasm_list`, `wasm_upload`, `wasm_control`, `wasm_info`, `wasm_stats`, `check_wasm_support`
- `start_server`, `stop_server`, `server_status`, `restart_server`, `server_logs`
- `provision_node`, `read_nvs`, `erase_nvs`, `validate_config`, `generate_mesh_configs`
- `get_settings`, `save_settings`

Source: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/src/lib.rs:10-45`

## Non-Negotiable Quality Bar for Redesign

- No emojis in product UI.
- No demo/mock/simulated data in primary production flows.
- Advanced settings grouped together in a dedicated section.
- Shadcn component system as the UI foundation.
- Old code retained untouched for reference.

