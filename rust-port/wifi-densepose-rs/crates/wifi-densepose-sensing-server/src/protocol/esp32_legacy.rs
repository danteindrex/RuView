//! Decoders for ESP32/Pi edge packet wire formats.
//!
//! Supports:
//! - Raw CSI frames (`0xC5110001`) with canonical and compatibility layouts.
//! - Edge vitals (`0xC5110002`) and fused vitals (`0xC5110004`).
//! - Feature vector packets (`0xC5110003`).
//! - Compressed frame packets (`0xC5110005`).
//! - ADR-081 feature state packets (`0xC5110006`, CRC32-gated).
//! - WASM output packets (legacy `0xC5110004`, `0xC5110006` when the CRC
//!   gate fails, and v2 `0xC5110007`).

use serde::Serialize;

use crate::types::{Esp32Frame, Esp32VitalsPacket, WasmEvent, WasmOutputPacket};

#[derive(Debug, Clone)]
pub struct Esp32FeaturePacket {
    pub node_id: u8,
    pub seq: u16,
    pub timestamp_us: i64,
    pub features: [f32; 8],
}

#[derive(Debug, Clone)]
pub struct Esp32CompressedPacket {
    pub node_id: u8,
    pub channel: u8,
    pub original_iq_len: u16,
    pub compressed_len: u16,
}

fn magic(buf: &[u8]) -> Option<u32> {
    if buf.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))
}

pub fn parse_esp32_frame(buf: &[u8]) -> Option<Esp32Frame> {
    if buf.len() < 20 || magic(buf)? != 0xC511_0001 {
        return None;
    }

    // Distinguish canonical layout vs historical compatibility layout.
    // Canonical format has freq_mhz u32 at [8..11], so bytes 10..11 are
    // normally zero for MHz ranges used by WiFi (< 65536).
    let likely_canonical = buf[10] == 0 && buf[11] == 0;
    if likely_canonical {
        parse_frame_canonical(buf).or_else(|| parse_frame_compat(buf))
    } else {
        parse_frame_compat(buf).or_else(|| parse_frame_canonical(buf))
    }
}

fn parse_frame_canonical(buf: &[u8]) -> Option<Esp32Frame> {
    let node_id = buf[4];
    let n_antennas = buf[5];
    let n_sub_u16 = u16::from_le_bytes([buf[6], buf[7]]);
    if n_antennas == 0 || n_sub_u16 == 0 {
        return None;
    }

    let freq_u32 = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let sequence = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let rssi_raw = buf[16] as i8;
    let noise_floor = buf[17] as i8;

    let n_pairs = n_antennas as usize * n_sub_u16 as usize;
    let expected_len = 20 + n_pairs * 2;
    if buf.len() < expected_len {
        return None;
    }

    let mut amplitudes = Vec::with_capacity(n_pairs);
    let mut phases = Vec::with_capacity(n_pairs);
    for k in 0..n_pairs {
        let i_val = buf[20 + k * 2] as i8 as f64;
        let q_val = buf[20 + k * 2 + 1] as i8 as f64;
        amplitudes.push((i_val * i_val + q_val * q_val).sqrt());
        phases.push(q_val.atan2(i_val));
    }

    let rssi = if rssi_raw > 0 {
        rssi_raw.saturating_neg()
    } else {
        rssi_raw
    };
    let freq_mhz = u16::try_from(freq_u32).unwrap_or(u16::MAX);
    // Widen to u16 for Nexmon compatibility (256 subcarriers at 80 MHz).
    let n_subcarriers = n_sub_u16;

    Some(Esp32Frame {
        magic: 0xC511_0001,
        node_id,
        n_antennas,
        n_subcarriers,
        freq_mhz,
        sequence,
        rssi,
        noise_floor,
        amplitudes,
        phases,
    })
}

fn parse_frame_compat(buf: &[u8]) -> Option<Esp32Frame> {
    let node_id = buf[4];
    let n_antennas = buf[5];
    let n_subcarriers_u8 = buf[6];
    if n_antennas == 0 || n_subcarriers_u8 == 0 {
        return None;
    }
    // Widen to u16 for compatibility with CsiFrame.
    let n_subcarriers = n_subcarriers_u8 as u16;

    let freq_mhz = u16::from_le_bytes([buf[8], buf[9]]);
    let sequence = u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]);
    let rssi_raw = buf[14] as i8;
    let noise_floor = buf[15] as i8;

    let n_pairs = n_antennas as usize * n_subcarriers_u8 as usize;
    let expected_len = 20 + n_pairs * 2;
    if buf.len() < expected_len {
        return None;
    }

    let mut amplitudes = Vec::with_capacity(n_pairs);
    let mut phases = Vec::with_capacity(n_pairs);
    for k in 0..n_pairs {
        let i_val = buf[20 + k * 2] as i8 as f64;
        let q_val = buf[20 + k * 2 + 1] as i8 as f64;
        amplitudes.push((i_val * i_val + q_val * q_val).sqrt());
        phases.push(q_val.atan2(i_val));
    }

    let rssi = if rssi_raw > 0 {
        rssi_raw.saturating_neg()
    } else {
        rssi_raw
    };

    Some(Esp32Frame {
        magic: 0xC511_0001,
        node_id,
        n_antennas,
        n_subcarriers,
        freq_mhz,
        sequence,
        rssi,
        noise_floor,
        amplitudes,
        phases,
    })
}

pub fn parse_esp32_vitals_or_fused(buf: &[u8]) -> Option<Esp32VitalsPacket> {
    if buf.len() < 32 {
        return None;
    }
    let m = magic(buf)?;
    if m != 0xC511_0002 && m != 0xC511_0004 {
        return None;
    }
    // `0xC5110004` can also be legacy WASM output. If it structurally
    // looks like WASM, do not treat it as fused vitals.
    if m == 0xC511_0004 && looks_like_legacy_wasm(buf) {
        return None;
    }

    let node_id = buf[4];
    let flags = buf[5];
    let breathing_raw = u16::from_le_bytes([buf[6], buf[7]]);
    let heartrate_raw = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
    let rssi = buf[12] as i8;
    let n_persons = buf[13];
    let motion_energy = f32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let presence_score = f32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
    let timestamp_ms = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);

    let mut pkt = Esp32VitalsPacket {
        node_id,
        presence: (flags & 0x01) != 0,
        fall_detected: (flags & 0x02) != 0,
        motion: (flags & 0x04) != 0,
        breathing_rate_bpm: breathing_raw as f64 / 100.0,
        heartrate_bpm: heartrate_raw as f64 / 10000.0,
        rssi,
        n_persons,
        motion_energy,
        presence_score,
        timestamp_ms,
        mmwave_hr_bpm: None,
        mmwave_br_bpm: None,
        mmwave_distance_cm: None,
        mmwave_targets: None,
        mmwave_confidence: None,
        fusion_confidence: None,
    };

    // ADR-063 fused-vitals mmWave extension (edge_fused_vitals_pkt_t in
    // firmware/esp32-csi-node/main/edge_processing.h): bytes 14/15 are
    // mmwave_type/fusion_confidence, 28..40 three f32s, 40/41 two u8s.
    if m == 0xC511_0004 && buf.len() >= 48 {
        pkt.fusion_confidence = Some(buf[15]);
        pkt.mmwave_hr_bpm = Some(f32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]));
        pkt.mmwave_br_bpm = Some(f32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]));
        pkt.mmwave_distance_cm = Some(f32::from_le_bytes([buf[36], buf[37], buf[38], buf[39]]));
        pkt.mmwave_targets = Some(buf[40]);
        pkt.mmwave_confidence = Some(buf[41]);
    }

    Some(pkt)
}

pub fn parse_esp32_wasm_output(buf: &[u8]) -> Option<WasmOutputPacket> {
    if buf.len() < 8 {
        return None;
    }
    let m = magic(buf)?;
    // 0xC5110007 is the WASM v2 magic (post ADR-081 firmware migration).
    // 0xC5110006 is accepted for old firmware, but only for payloads that
    // fail the rv_feature_state CRC gate — the caller must try
    // `parse_rv_feature_state` first.
    if m == 0xC511_0007 || m == 0xC511_0006 {
        return parse_wasm_payload(buf);
    }
    if m == 0xC511_0004 && looks_like_legacy_wasm(buf) {
        return parse_wasm_payload(buf);
    }
    None
}

fn parse_wasm_payload(buf: &[u8]) -> Option<WasmOutputPacket> {
    let node_id = buf[4];
    let module_id = buf[5];
    let event_count = u16::from_le_bytes([buf[6], buf[7]]) as usize;
    if event_count > 64 {
        return None;
    }
    let expected_len = 8 + event_count * 5;
    if buf.len() < expected_len {
        return None;
    }

    let mut events = Vec::with_capacity(event_count);
    let mut offset = 8usize;
    for _ in 0..event_count {
        let event_type = buf[offset];
        let value = f32::from_le_bytes([
            buf[offset + 1],
            buf[offset + 2],
            buf[offset + 3],
            buf[offset + 4],
        ]);
        events.push(WasmEvent { event_type, value });
        offset += 5;
    }

    Some(WasmOutputPacket {
        node_id,
        module_id,
        events,
    })
}

pub fn parse_esp32_feature_packet(buf: &[u8]) -> Option<Esp32FeaturePacket> {
    if buf.len() < 48 || magic(buf)? != 0xC511_0003 {
        return None;
    }

    let node_id = buf[4];
    let seq = u16::from_le_bytes([buf[6], buf[7]]);
    let timestamp_us = i64::from_le_bytes([
        buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
    ]);

    let mut features = [0.0f32; 8];
    let mut off = 16usize;
    for slot in &mut features {
        *slot = f32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
        off += 4;
    }

    Some(Esp32FeaturePacket {
        node_id,
        seq,
        timestamp_us,
        features,
    })
}

pub fn parse_esp32_compressed_packet(buf: &[u8]) -> Option<Esp32CompressedPacket> {
    if buf.len() < 10 || magic(buf)? != 0xC511_0005 {
        return None;
    }

    let node_id = buf[4];
    let channel = buf[5];
    let original_iq_len = u16::from_le_bytes([buf[6], buf[7]]);
    let compressed_len = u16::from_le_bytes([buf[8], buf[9]]);
    let expected_len = 10 + compressed_len as usize;
    if buf.len() < expected_len {
        return None;
    }

    Some(Esp32CompressedPacket {
        node_id,
        channel,
        original_iq_len,
        compressed_len,
    })
}

fn looks_like_legacy_wasm(buf: &[u8]) -> bool {
    if buf.len() < 8 {
        return false;
    }
    let event_count = u16::from_le_bytes([buf[6], buf[7]]) as usize;
    if event_count > 64 {
        return false;
    }
    // WASM output packets are exactly 8 + 5*event_count bytes.
    if buf.len() != 8 + event_count * 5 {
        return false;
    }
    // The firmware only has 4 WASM module slots; byte 5 is module_id in
    // the WASM layout but flags in the fused-vitals layout.
    if buf[5] >= 4 {
        return false;
    }
    // A 48-byte packet is ambiguous (event_count 8 == fused-vitals size).
    // Prefer fused vitals when byte 5 uses only flag bits 0-3
    // (presence/fall/motion/mmwave_present).
    if buf.len() == 48 && buf[5] & 0xF0 == 0 {
        return false;
    }
    true
}

// ── ADR-081 feature state packet (magic 0xC5110006, 60 bytes, CRC-gated) ────

/// Wire size of `rv_feature_state_t` (firmware rv_feature_state.h).
pub const RV_FEATURE_STATE_LEN: usize = 60;

/// Decoded ADR-081 compact per-node sensing state
/// (`rv_feature_state_t` in firmware/esp32-csi-node/main/rv_feature_state.h).
#[derive(Debug, Clone, Serialize)]
pub struct RvFeatureState {
    pub node_id: u8,
    /// rv_capture_profile_t of the dominant window at emit time.
    pub mode: u8,
    pub seq: u16,
    pub ts_us: u64,
    pub motion_score: f32,
    pub presence_score: f32,
    pub respiration_bpm: f32,
    pub respiration_conf: f32,
    pub heartbeat_bpm: f32,
    pub heartbeat_conf: f32,
    pub anomaly_score: f32,
    pub env_shift_score: f32,
    pub node_coherence: f32,
    pub quality_flags: u16,
}

/// IEEE CRC32 (reflected, poly 0xEDB88320), matching the firmware's
/// `rv_feature_state_crc32`.
pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Parse an ADR-081 feature state packet. Gate: magic + exact length + CRC32
/// over bytes [0..56] matching bytes [56..60]. On CRC failure returns None so
/// the payload can fall through to the legacy WASM parser (old firmware
/// shared the 0xC5110006 magic).
pub fn parse_rv_feature_state(buf: &[u8]) -> Option<RvFeatureState> {
    if buf.len() != RV_FEATURE_STATE_LEN || magic(buf)? != 0xC511_0006 {
        return None;
    }
    let stored_crc = u32::from_le_bytes([buf[56], buf[57], buf[58], buf[59]]);
    if crc32_ieee(&buf[..56]) != stored_crc {
        return None;
    }

    let f32_at = |off: usize| f32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);

    Some(RvFeatureState {
        node_id: buf[4],
        mode: buf[5],
        seq: u16::from_le_bytes([buf[6], buf[7]]),
        ts_us: u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]),
        motion_score: f32_at(16),
        presence_score: f32_at(20),
        respiration_bpm: f32_at(24),
        respiration_conf: f32_at(28),
        heartbeat_bpm: f32_at(32),
        heartbeat_conf: f32_at(36),
        anomaly_score: f32_at(40),
        env_shift_score: f32_at(44),
        node_coherence: f32_at(48),
        quality_flags: u16::from_le_bytes([buf[52], buf[53]]),
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_ieee_check_value() {
        // Standard CRC-32/ISO-HDLC check value for "123456789".
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    }

    /// Golden 60-byte rv_feature_state_t per rv_feature_state.h.
    fn golden_feature_state() -> Vec<u8> {
        let mut p = Vec::with_capacity(60);
        p.extend_from_slice(&0xC511_0006u32.to_le_bytes()); // magic
        p.push(3); // node_id
        p.push(1); // mode
        p.extend_from_slice(&0x00ABu16.to_le_bytes()); // seq
        p.extend_from_slice(&123_456_789u64.to_le_bytes()); // ts_us
        for v in [0.25f32, 0.9, 14.5, 0.8, 62.0, 0.6, 0.05, 0.1, 0.95] {
            p.extend_from_slice(&v.to_le_bytes());
        }
        p.extend_from_slice(&0x0007u16.to_le_bytes()); // quality_flags
        p.extend_from_slice(&0u16.to_le_bytes()); // reserved
        let crc = crc32_ieee(&p);
        p.extend_from_slice(&crc.to_le_bytes());
        assert_eq!(p.len(), RV_FEATURE_STATE_LEN);
        p
    }

    #[test]
    fn parses_golden_feature_state() {
        let pkt = parse_rv_feature_state(&golden_feature_state()).expect("must parse");
        assert_eq!(pkt.node_id, 3);
        assert_eq!(pkt.mode, 1);
        assert_eq!(pkt.seq, 0x00AB);
        assert_eq!(pkt.ts_us, 123_456_789);
        assert!((pkt.motion_score - 0.25).abs() < 1e-6);
        assert!((pkt.presence_score - 0.9).abs() < 1e-6);
        assert!((pkt.respiration_bpm - 14.5).abs() < 1e-6);
        assert!((pkt.respiration_conf - 0.8).abs() < 1e-6);
        assert!((pkt.heartbeat_bpm - 62.0).abs() < 1e-6);
        assert!((pkt.heartbeat_conf - 0.6).abs() < 1e-6);
        assert!((pkt.anomaly_score - 0.05).abs() < 1e-6);
        assert!((pkt.env_shift_score - 0.1).abs() < 1e-6);
        assert!((pkt.node_coherence - 0.95).abs() < 1e-6);
        assert_eq!(pkt.quality_flags, 0x0007);
    }

    #[test]
    fn feature_state_rejected_on_bad_crc_falls_to_wasm_parser() {
        let mut p = golden_feature_state();
        p[57] ^= 0xFF; // corrupt CRC
        assert!(parse_rv_feature_state(&p).is_none());
        // Legacy firmware WASM packets share the 0xC5110006 magic; the CRC
        // failure must leave the payload available to the WASM parser.
        assert!(parse_esp32_wasm_output(&p).is_some());
    }

    #[test]
    fn feature_state_rejected_on_wrong_length() {
        let mut p = golden_feature_state();
        p.push(0);
        assert!(parse_rv_feature_state(&p).is_none());
    }

    #[test]
    fn wasm_v2_magic_0xc5110007_accepted() {
        let mut p = Vec::new();
        p.extend_from_slice(&0xC511_0007u32.to_le_bytes());
        p.push(2); // node_id
        p.push(1); // module_id
        p.extend_from_slice(&2u16.to_le_bytes()); // event_count
        for (t, v) in [(1u8, 0.5f32), (3u8, 42.0f32)] {
            p.push(t);
            p.extend_from_slice(&v.to_le_bytes());
        }
        let pkt = parse_esp32_wasm_output(&p).expect("v2 magic must parse");
        assert_eq!(pkt.node_id, 2);
        assert_eq!(pkt.module_id, 1);
        assert_eq!(pkt.events.len(), 2);
        assert_eq!(pkt.events[1].event_type, 3);
    }

    /// Golden 48-byte edge_fused_vitals_pkt_t per edge_processing.h.
    fn golden_fused_vitals() -> Vec<u8> {
        let mut p = Vec::with_capacity(48);
        p.extend_from_slice(&0xC511_0004u32.to_le_bytes()); // magic
        p.push(7); // node_id
        p.push(0x0D); // flags: presence | motion | mmwave_present
        p.extend_from_slice(&1450u16.to_le_bytes()); // breathing_rate (14.5 * 100)
        p.extend_from_slice(&720_000u32.to_le_bytes()); // heartrate (72.0 * 10000)
        p.push((-55i8) as u8); // rssi
        p.push(1); // n_persons
        p.push(2); // mmwave_type
        p.push(90); // fusion_confidence
        p.extend_from_slice(&0.4f32.to_le_bytes()); // motion_energy
        p.extend_from_slice(&0.85f32.to_le_bytes()); // presence_score
        p.extend_from_slice(&100_000u32.to_le_bytes()); // timestamp_ms
        p.extend_from_slice(&71.5f32.to_le_bytes()); // mmwave_hr_bpm
        p.extend_from_slice(&14.2f32.to_le_bytes()); // mmwave_br_bpm
        p.extend_from_slice(&215.0f32.to_le_bytes()); // mmwave_distance (cm)
        p.push(1); // mmwave_targets
        p.push(80); // mmwave_confidence
        p.extend_from_slice(&0u16.to_le_bytes()); // reserved3
        p.extend_from_slice(&0u32.to_le_bytes()); // reserved4
        assert_eq!(p.len(), 48);
        p
    }

    #[test]
    fn fused_vitals_48b_prefers_vitals_and_parses_mmwave_extension() {
        let p = golden_fused_vitals();
        // Must NOT be treated as a legacy WASM packet (48-byte ambiguity).
        assert!(
            parse_esp32_wasm_output(&p).is_none(),
            "48-byte packet with flag bits 0-3 must resolve to fused vitals"
        );
        let v = parse_esp32_vitals_or_fused(&p).expect("must parse as fused vitals");
        assert_eq!(v.node_id, 7);
        assert!(v.presence);
        assert!(!v.fall_detected);
        assert!(v.motion);
        assert!((v.breathing_rate_bpm - 14.5).abs() < 1e-9);
        assert!((v.heartrate_bpm - 72.0).abs() < 1e-9);
        assert_eq!(v.rssi, -55);
        assert_eq!(v.n_persons, 1);
        assert_eq!(v.fusion_confidence, Some(90));
        assert!((v.mmwave_hr_bpm.unwrap() - 71.5).abs() < 1e-6);
        assert!((v.mmwave_br_bpm.unwrap() - 14.2).abs() < 1e-6);
        assert!((v.mmwave_distance_cm.unwrap() - 215.0).abs() < 1e-6);
        assert_eq!(v.mmwave_targets, Some(1));
        assert_eq!(v.mmwave_confidence, Some(80));
    }

    #[test]
    fn plain_vitals_32b_has_no_mmwave_extension() {
        let mut p = golden_fused_vitals()[..32].to_vec();
        p[0..4].copy_from_slice(&0xC511_0002u32.to_le_bytes());
        let v = parse_esp32_vitals_or_fused(&p).expect("must parse as plain vitals");
        assert!(v.mmwave_hr_bpm.is_none());
        assert!(v.fusion_confidence.is_none());
    }

    #[test]
    fn legacy_wasm_on_0xc5110004_requires_exact_length() {
        // Well-formed legacy WASM: module_id 1, 2 events, exactly 18 bytes.
        let mut p = Vec::new();
        p.extend_from_slice(&0xC511_0004u32.to_le_bytes());
        p.push(5); // node_id
        p.push(1); // module_id
        p.extend_from_slice(&2u16.to_le_bytes());
        for _ in 0..2 {
            p.push(1);
            p.extend_from_slice(&1.0f32.to_le_bytes());
        }
        assert!(parse_esp32_wasm_output(&p).is_some());
        assert!(parse_esp32_vitals_or_fused(&p).is_none(), "18-byte WASM is too short for vitals");

        // Trailing byte breaks the exact-length rule -> not WASM.
        p.push(0);
        assert!(parse_esp32_wasm_output(&p).is_none());
    }

    #[test]
    fn legacy_wasm_rejects_module_id_out_of_range() {
        let mut p = Vec::new();
        p.extend_from_slice(&0xC511_0004u32.to_le_bytes());
        p.push(5); // node_id
        p.push(9); // module_id >= 4: invalid
        p.extend_from_slice(&1u16.to_le_bytes());
        p.push(1);
        p.extend_from_slice(&1.0f32.to_le_bytes());
        assert!(parse_esp32_wasm_output(&p).is_none());
    }
}
