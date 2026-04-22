//! Decoders for ESP32/Pi edge packet wire formats.
//!
//! Supports:
//! - Raw CSI frames (`0xC5110001`) with canonical and compatibility layouts.
//! - Edge vitals (`0xC5110002`) and fused vitals (`0xC5110004`).
//! - Feature vector packets (`0xC5110003`).
//! - Compressed frame packets (`0xC5110005`).
//! - WASM output packets (legacy `0xC5110004` and v2 `0xC5110006`).

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

    Some(Esp32VitalsPacket {
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
    })
}

pub fn parse_esp32_wasm_output(buf: &[u8]) -> Option<WasmOutputPacket> {
    if buf.len() < 8 {
        return None;
    }
    let m = magic(buf)?;
    if m == 0xC511_0006 {
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
    let expected = 8 + event_count * 5;
    expected <= buf.len()
}

