# Production UI Redesign Plan (New Files Only)

Date: 2026-04-22  
Baseline prompt: `plans/ui-redesign-2026-04-22/user-prompt-verification.md`

## Objective

Redesign the desktop UI from scratch to production quality while preserving all existing functionality and keeping legacy code untouched for reference.

## Guardrails

- Do not modify existing UI implementation files.
- Build redesign in a new code location.
- Keep command-level functionality parity with current Tauri invoke handlers.
- Remove demo/simulated behavior from primary production flows.
- Use shadcn components as the default building blocks.
- Group advanced settings in one dedicated area.

## Assumed Target Surface

Primary target:
- `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui`

Legacy `ui/` stays reference-only in this phase.

## Proposed New Workspace (No Legacy Edits)

Create:
- `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/`

Planned structure:
- `ui-v2/src/app/` (routing, shell, providers)
- `ui-v2/src/components/ui/` (shadcn primitives)
- `ui-v2/src/components/layout/` (sidebar, topbar, page frames)
- `ui-v2/src/components/domain/` (feature widgets)
- `ui-v2/src/features/discovery/`
- `ui-v2/src/features/flash/`
- `ui-v2/src/features/ota/`
- `ui-v2/src/features/modules/`
- `ui-v2/src/features/sensing/`
- `ui-v2/src/features/mesh3d/`
- `ui-v2/src/features/settings/advanced/`
- `ui-v2/src/services/tauri/` (typed invoke clients)
- `ui-v2/src/state/` (query/cache/store)
- `ui-v2/src/theme/` (tokens, semantic vars)
- `ui-v2/src/tests/`

## Shadcn Component Baseline

Core:
- `button`, `input`, `select`, `textarea`, `switch`, `checkbox`, `radio-group`
- `card`, `tabs`, `separator`, `badge`, `tooltip`, `dialog`, `sheet`
- `dropdown-menu`, `context-menu`, `popover`, `command`
- `table`, `scroll-area`, `accordion`, `collapsible`, `alert`, `alert-dialog`
- `progress`, `skeleton`, `sonner`/toast

Operations-specific:
- `data table` pattern with column visibility, sorting, filtering
- grouped form sections with `accordion` for advanced settings
- destructive actions isolated in `alert-dialog` confirmation flows

## Feature Parity Matrix

Must preserve command coverage:
- Discovery: `discover_nodes`, `list_serial_ports`, `configure_esp32_wifi`
- Flash: `flash_firmware`, `flash_progress`, `verify_firmware`, `check_espflash`, `supported_chips`
- OTA: `ota_update`, `batch_ota_update`, `check_ota_endpoint`
- WASM: `wasm_list`, `wasm_upload`, `wasm_control`, `wasm_info`, `wasm_stats`, `check_wasm_support`
- Server: `start_server`, `stop_server`, `server_status`, `restart_server`, `server_logs`
- Provision: `provision_node`, `read_nvs`, `erase_nvs`, `validate_config`, `generate_mesh_configs`
- Settings: `get_settings`, `save_settings`

Source: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/src/lib.rs`

## UX Information Architecture (Production)

Top-level navigation:
1. Overview
2. Network
3. Device Operations
4. Edge Modules
5. Sensing
6. 3D Pose View
7. Settings

Advanced settings grouping:
- Settings > Advanced
  - Network + transport
  - OTA safety + rollout policy
  - Server runtime + logs
  - Diagnostics and recovery actions

No advanced toggles scattered across unrelated screens.

## 3D Pose View Redesign (Not “Vibe Coded”)

Replace current random graph with deterministic pose renderer:
- Input: MediaPipe-style world landmarks (`x,y,z`, confidence)
- Skeleton: canonical connector topology
- Rendering:
  - confidence-based opacity/line thickness
  - axis helper, fixed scale, view presets
  - temporal smoothing and outlier rejection
- Optional phase 2:
  - rig retarget preview (Three.js `SkeletonHelper`, bone map)

References in:
- `plans/ui-redesign-2026-04-22/3d-view-research-notes.md`

## Delivery Phases

### Phase 1: Foundation
- Scaffold `ui-v2` with React + TypeScript + Tailwind + shadcn.
- Build app shell and tokenized theme.
- Add typed Tauri service layer for all command groups.

### Phase 2: Core Operations Screens
- Build Overview, Network, Device Operations, Settings.
- Enforce no simulation modes in production workflows.
- Add grouped Advanced Settings section.

### Phase 3: Edge Modules + Sensing
- Rebuild module lifecycle UI with table-driven operations.
- Rebuild sensing screen with strict source-state semantics and clear health/status.

### Phase 4: 3D Pose View
- Implement deterministic 3D landmark skeleton viewer.
- Add confidence gating, controls, and diagnostics overlays.

### Phase 5: Hardening
- Keyboard accessibility, focus order, contrast checks, loading/error states.
- End-to-end smoke tests for all command paths.
- Migration validation against parity checklist.

## Acceptance Checklist

- No emojis in product UI.
- No demo/simulated primary mode exposed to end users.
- Advanced settings grouped in one location.
- All command pathways available and verified.
- Legacy UI untouched.
- New UI can be built and run independently.

## Risks and Controls

1. Risk: parity regressions while replacing large monolithic pages.  
Control: command-by-command parity tests and signed checklist.

2. Risk: 3D viewer performance issues.  
Control: frame throttling, worker/off-main-thread processing strategy, confidence pruning.

3. Risk: inconsistent state handling.  
Control: central query/state layer and strict domain DTO typing.

