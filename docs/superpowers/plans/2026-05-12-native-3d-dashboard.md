# Native 3D Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the iframe-based 3D pose page with a native observatory mount inside `ui-v2`, then make it the default desktop dashboard while preserving current 3D features.

**Architecture:** Reuse the existing `public/observatory` runtime instead of rewriting the renderer. Patch that runtime so it can mount into a container inside the React app, then create a React host page that renders the observatory DOM directly and passes live WebSocket settings from desktop app state.

**Tech Stack:** Tauri v2, React, TypeScript, Vite, Tailwind, standalone ESM observatory modules, Three.js via import map

---

### Task 1: Patch Observatory Runtime For Container Mounting

**Files:**
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/public/observatory/js/main.js`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/public/observatory/css/observatory.css`

- [ ] Add an exported `mountObservatory(root, options)` entry point and keep legacy auto-boot support for `observatory.html`.
- [ ] Replace full-window sizing assumptions with container sizing.
- [ ] Add a `destroy()` cleanup path for resize listeners, keyboard listeners, reconnect timers, and WebSocket teardown.
- [ ] Keep all existing HUD and rendering behavior unchanged unless required for container mounting.

### Task 2: Create React Observatory Host

**Files:**
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/src/components/observatory/observatory-markup.tsx`
- Create: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/src/components/observatory/observatory-host.tsx`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/src/pages/pose3d-page.tsx`

- [ ] Render the observatory HTML structure directly in React with the same IDs expected by the existing runtime.
- [ ] Inject the observatory import map once before loading `/observatory/js/main.js`.
- [ ] Mount and unmount the observatory runtime from React.
- [ ] Pass mode and `wsUrl` from Tauri server status instead of iframe query params.

### Task 3: Promote 3D Pose To Main Dashboard

**Files:**
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/src/App.tsx`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/src/components/layout/app-shell.tsx`

- [ ] Change the default active page from `dashboard` to `pose3d`.
- [ ] Move `3D Pose` to the top of primary navigation.
- [ ] Keep the existing overview/admin pages intact and reachable.
- [ ] Update title/subtitle copy so the primary dashboard reads as the main experience.

### Task 4: Add Light Theme Integration For Observatory HUD

**Files:**
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/public/observatory/css/observatory.css`
- Modify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2/src/styles.css`

- [ ] Scope observatory CSS under a mount root so it can coexist with app styling.
- [ ] Add theme-aware CSS variables for observatory panels, text, borders, and controls.
- [ ] Ensure light theme avoids hard black backgrounds and remains readable.
- [ ] Keep the immersive visual character without breaking desktop app consistency.

### Task 5: Verification

**Files:**
- Verify: `rust-port/wifi-densepose-rs/crates/wifi-densepose-desktop/ui-v2`

- [ ] Run the frontend build for `ui-v2`.
- [ ] Verify there is no iframe in `pose3d-page.tsx`.
- [ ] Verify the app defaults to `pose3d`.
- [ ] Verify theme toggling still works and the observatory mount renders in both themes.
- [ ] Verify existing non-3D pages still compile.
