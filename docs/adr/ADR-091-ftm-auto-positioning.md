# ADR-091: FTM Auto-Positioning for ESP32-S3 Fleets

- Status: Accepted (user decision 2026-07-05) — implementation in progress, **HARDWARE VALIDATION PENDING**
- Date: 2026-07-05

## Context

Node placement in Wave is currently manual: the drag-UI shipped, so operators
can position nodes on the floor plan by hand, but nothing measures where nodes
actually are. Multistatic fusion quality (ADR-029/ADR-030) and tomography
(voxel grid) degrade quickly with placement error, and asking operators to
tape-measure every node does not scale.

802.11 FTM (Fine Timing Measurement, IEEE 802.11-2016 / 802.11mc) gives
RTT-based node-to-node distance estimates with roughly meter-level accuracy on
ESP32 hardware. The ESP32-S3 WiFi driver supports both FTM initiator and FTM
responder roles (responder only from SoftAP mode). A set of pairwise distances
solves the *shape* of the node constellation, not its *frame*: the solution is
invariant under rotation, translation, and reflection, so anchoring to the room
remains a (light) manual step.

## Decision

**FTM is the primary auto-positioning mechanism for ESP32-S3 fleets, with
room-anchoring optional** (user decision 2026-07-05). The server commands
ranging on demand; nodes measure and report; the server solves the layout.

### 1. Wire contracts

Control messages ride the existing discovery socket (UDP :5006, ASCII,
`discovery_responder.c`):

| Message | Action |
|---------|--------|
| `WAVE_RANGE\|AA:BB:CC:DD:EE:FF` | Initiate an FTM session to that peer MAC on the current channel. Non-blocking; nothing is acked on :5006 — the result flows to the aggregator as the range report below. |
| `WAVE_FTM_RESPONDER\|on` / `WAVE_FTM_RESPONDER\|off` | Enable/disable FTM responder mode (persisted to NVS key `ftm_resp`). |

Range report (node → aggregator, UDP :5005, exactly 24 bytes, little-endian,
magic `0xC5110008` — next in the `0xC5110001`–`0xC5110007` family):

| Offset | Field | Type | Notes |
|--------|-------|------|-------|
| 0 | magic | u32 | `0xC5110008` |
| 4 | node_id | u8 | Reporting (initiator) node |
| 5 | status | u8 | 0 = ok, 1 = session failed/timeout, 2 = unsupported/responder-off-peer, 3 = busy |
| 6 | peer MAC | [6]u8 | Ranged peer |
| 12 | distance_cm | u32 | 0 when status != 0 |
| 16 | rtt_est_ns | u32 | Driver RTT estimate |
| 20 | num_frames | u8 | FTM frames used |
| 21 | reserved | [3]u8 | 0 |

### 2. Firmware implementation (`firmware/esp32-csi-node/main/ftm_ranging.c/.h`)

- **Initiator**: `esp_wifi_ftm_initiate_session()` on the current channel,
  `frm_count=16`, `burst_period=200 ms`. Result harvested from
  `WIFI_EVENT_FTM_REPORT` (`rtt_est`, `dist_est`), forwarded via the existing
  `stream_sender` socket. Sessions are serialized — one in flight, status 3
  (busy) reported otherwise — with a 4 s timeout guard (status 1).
- **Responder**: ESP-IDF answers FTM only from SoftAP, so enabling the
  responder switches WIFI_MODE_STA → WIFI_MODE_APSTA with a hidden,
  random-passworded WPA2 SoftAP (SSID `RV-FTM-<node_id>`, `ftm_responder=true`)
  on the STA channel. Refused while ADR-073 channel hopping is active (a
  SoftAP cannot follow the hop table).
- **Responder default: OFF (on-demand).** Kconfig `CONFIG_ESP_WIFI_FTM_ENABLE=y`
  is added to all sdkconfig defaults; NVS key `ftm_resp` (u8, default 0) opts a
  node into responder-at-boot.
- Log tag `ftm_ranging` covers session start / report / failure for hardware
  validation.

### 3. Server-side solving (server agent, out of scope for the firmware)

- **N = 2**: a single distance is degenerate — place the two nodes on a line
  at the measured separation (arbitrary orientation).
- **N ≥ 3**: classical MDS (multidimensional scaling) over the pairwise
  distance matrix yields 2D coordinates up to rotation/translation/reflection.
- **Room anchoring is optional**: the operator may pin 1–2 nodes on the
  drag-UI to fix the frame; unanchored fleets still get a correct relative
  shape.

## Coexistence risk

The node's primary job is continuous CSI capture in STA mode. The responder's
STA → APSTA switch shares the single radio, and whether the CSI callback and
its channel survive the mode switch untouched has **not** been proven on
hardware. ESP-IDF documentation does not guarantee CSI/APSTA coexistence
semantics across the switch, so we choose the conservative default:

- Responder **OFF by default**, enabled on demand per node only while a
  positioning sweep runs, then turned back off.
- Initiator sessions are short (16 frames on the current channel) and bounded
  by the 4 s guard, minimizing CSI interruption.

### Validation plan (pending)

1. Flash two ESP32-S3 boards with this firmware.
2. **Stock accuracy test**: responder on board A, `WAVE_RANGE` from board B
   at known separations (1/3/5 m); record `distance_cm` spread.
3. **Integrated test**: verify CSI frame rate (`0xC5110001` at the aggregator)
   before, during, and after (a) an initiator session and (b) a responder
   on → off cycle. Any sustained CSI dropout blocks default-on responder.

## Pi exclusion

Raspberry Pi 4 nodes (ADR-090, Nexmon CSI) are excluded: the Nexmon-patched
brcmfmac firmware has **no FTM support**. Mixed fleets position their ESP32-S3
members via FTM; Pi nodes remain manually placed (or anchored relative to
ranged ESP32 nodes by the operator).

## Consequences

- ESP32-S3 fleets self-measure their constellation shape on server command;
  manual placement drops to an optional anchoring step.
- New packet magic `0xC5110008` must be parsed by the sensing server
  (`protocol/` modules — server agent's contract).
- Responder mode temporarily broadcasts a hidden SoftAP; it is WPA2 with a
  random throwaway password and nothing is expected to associate.
- Default-on responder is deferred until the validation plan above passes on
  real hardware.
