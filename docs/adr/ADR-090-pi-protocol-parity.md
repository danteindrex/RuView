# ADR-090: Raspberry Pi Protocol Parity

- Status: Accepted
- Date: 2026-04-22

## Context

RuView previously assumed ESP32-origin packets for end-to-end sensing. We now need full feature parity from Raspberry Pi 4 nodes running Nexmon CSI while keeping existing UI/API contracts stable.

## Decision

1. Canonical packet IDs are fixed and unique:
   - `0xC5110001` raw CSI frame
   - `0xC5110002` edge vitals
   - `0xC5110003` edge feature vector
   - `0xC5110004` fused vitals (legacy `0xC5110004` WASM is still parsed for compatibility)
   - `0xC5110005` compressed frame
   - `0xC5110006` WASM v2 events
2. Sensing server parsing is split into explicit protocol modules:
   - `protocol/packet.rs`
   - `protocol/esp32_legacy.rs`
   - `protocol/nexmon.rs`
3. Raw CSI frame header is canonicalized to use:
   - `n_subcarriers` as `u16` at bytes `[6..7]`
   - `freq_mhz` as `u32` at `[8..11]`
   - `sequence` as `u32` at `[12..15]`
4. Legacy shifted frame layout is supported as compatibility-only decode path.
5. Raspberry Pi node agent is the primary production path. The Python bridge remains compatibility-only.

## Consequences

- Existing downstream WebSocket and REST consumers remain unchanged.
- ESP32 and Pi/Nexmon can coexist in the same ingest path.
- Operators can migrate incrementally from ESP32-first deployments to Pi-first deployments without protocol breakage.
