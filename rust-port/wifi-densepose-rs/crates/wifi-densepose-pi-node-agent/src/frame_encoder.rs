use serde::{Deserialize, Serialize};

pub const MAGIC_RAW_FRAME: u32 = 0xC511_0001;
pub const MAGIC_VITALS: u32 = 0xC511_0002;
pub const MAGIC_FEATURE: u32 = 0xC511_0003;
pub const MAGIC_FUSED_VITALS: u32 = 0xC511_0004;
pub const MAGIC_COMPRESSED: u32 = 0xC511_0005;
pub const MAGIC_WASM_V2: u32 = 0xC511_0006;

#[derive(Debug, Clone)]
pub struct RawFrame {
    pub node_id: u8,
    pub n_antennas: u8,
    pub n_subcarriers: u16,
    pub freq_mhz: u16,
    pub sequence: u32,
    pub rssi: i8,
    pub noise_floor: i8,
    pub iq: Vec<(i8, i8)>,
}

impl RawFrame {
    pub fn amplitudes(&self) -> Vec<f32> {
        self.iq
            .iter()
            .map(|(i, q)| {
                let i = *i as f32;
                let q = *q as f32;
                (i * i + q * q).sqrt()
            })
            .collect()
    }

    pub fn mean_amplitude(&self) -> f32 {
        let amps = self.amplitudes();
        if amps.is_empty() {
            return 0.0;
        }
        amps.iter().sum::<f32>() / amps.len() as f32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeVitals {
    pub node_id: u8,
    pub presence: bool,
    pub fall_detected: bool,
    pub motion: bool,
    pub breathing_rate_bpm: f32,
    pub heartrate_bpm: f32,
    pub rssi: i8,
    pub n_persons: u8,
    pub motion_energy: f32,
    pub presence_score: f32,
    pub timestamp_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusedVitals {
    pub node_id: u8,
    pub presence: bool,
    pub fall_detected: bool,
    pub motion: bool,
    pub breathing_rate_bpm: f32,
    pub heartrate_bpm: f32,
    pub rssi: i8,
    pub n_persons: u8,
    pub motion_energy: f32,
    pub presence_score: f32,
    pub timestamp_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmEvent {
    pub event_type: u8,
    pub value: f32,
}

pub fn decode_magic(packet: &[u8]) -> Option<u32> {
    if packet.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([
        packet[0], packet[1], packet[2], packet[3],
    ]))
}

pub fn encode_raw_frame(frame: &RawFrame) -> Vec<u8> {
    let mut out = vec![0u8; 20 + frame.iq.len() * 2];
    out[0..4].copy_from_slice(&MAGIC_RAW_FRAME.to_le_bytes());
    out[4] = frame.node_id;
    out[5] = frame.n_antennas;
    out[6..8].copy_from_slice(&frame.n_subcarriers.to_le_bytes());
    out[8..12].copy_from_slice(&(frame.freq_mhz as u32).to_le_bytes());
    out[12..16].copy_from_slice(&frame.sequence.to_le_bytes());
    out[16] = frame.rssi as u8;
    out[17] = frame.noise_floor as u8;

    let mut off = 20usize;
    for (i, q) in &frame.iq {
        out[off] = *i as u8;
        out[off + 1] = *q as u8;
        off += 2;
    }
    out
}

pub fn encode_vitals_packet(vitals: &EdgeVitals) -> Vec<u8> {
    let mut out = vec![0u8; 32];
    out[0..4].copy_from_slice(&MAGIC_VITALS.to_le_bytes());
    out[4] = vitals.node_id;
    let mut flags = 0u8;
    if vitals.presence {
        flags |= 0x01;
    }
    if vitals.fall_detected {
        flags |= 0x02;
    }
    if vitals.motion {
        flags |= 0x04;
    }
    out[5] = flags;
    let br = (vitals.breathing_rate_bpm.max(0.0) * 100.0).round() as u16;
    let hr = (vitals.heartrate_bpm.max(0.0) * 10000.0).round() as u32;
    out[6..8].copy_from_slice(&br.to_le_bytes());
    out[8..12].copy_from_slice(&hr.to_le_bytes());
    out[12] = vitals.rssi as u8;
    out[13] = vitals.n_persons;
    out[16..20].copy_from_slice(&vitals.motion_energy.to_le_bytes());
    out[20..24].copy_from_slice(&vitals.presence_score.to_le_bytes());
    out[24..28].copy_from_slice(&vitals.timestamp_ms.to_le_bytes());
    out
}

pub fn encode_feature_packet(
    node_id: u8,
    seq: u16,
    timestamp_us: i64,
    features: [f32; 8],
) -> Vec<u8> {
    let mut out = vec![0u8; 48];
    out[0..4].copy_from_slice(&MAGIC_FEATURE.to_le_bytes());
    out[4] = node_id;
    out[6..8].copy_from_slice(&seq.to_le_bytes());
    out[8..16].copy_from_slice(&timestamp_us.to_le_bytes());
    let mut off = 16usize;
    for value in features {
        out[off..off + 4].copy_from_slice(&value.to_le_bytes());
        off += 4;
    }
    out
}

pub fn encode_compressed_packet(
    node_id: u8,
    channel: u8,
    original_iq_len: u16,
    payload: &[u8],
) -> Vec<u8> {
    let mut out = vec![0u8; 10 + payload.len()];
    out[0..4].copy_from_slice(&MAGIC_COMPRESSED.to_le_bytes());
    out[4] = node_id;
    out[5] = channel;
    out[6..8].copy_from_slice(&original_iq_len.to_le_bytes());
    out[8..10].copy_from_slice(&(payload.len() as u16).to_le_bytes());
    out[10..].copy_from_slice(payload);
    out
}

pub fn encode_fused_vitals_packet(vitals: &FusedVitals) -> Vec<u8> {
    let mut out = vec![0u8; 32];
    out[0..4].copy_from_slice(&MAGIC_FUSED_VITALS.to_le_bytes());
    out[4] = vitals.node_id;
    let mut flags = 0u8;
    if vitals.presence {
        flags |= 0x01;
    }
    if vitals.fall_detected {
        flags |= 0x02;
    }
    if vitals.motion {
        flags |= 0x04;
    }
    out[5] = flags;
    let br = (vitals.breathing_rate_bpm.max(0.0) * 100.0).round() as u16;
    let hr = (vitals.heartrate_bpm.max(0.0) * 10000.0).round() as u32;
    out[6..8].copy_from_slice(&br.to_le_bytes());
    out[8..12].copy_from_slice(&hr.to_le_bytes());
    out[12] = vitals.rssi as u8;
    out[13] = vitals.n_persons;
    out[16..20].copy_from_slice(&vitals.motion_energy.to_le_bytes());
    out[20..24].copy_from_slice(&vitals.presence_score.to_le_bytes());
    out[24..28].copy_from_slice(&vitals.timestamp_ms.to_le_bytes());
    out
}

pub fn encode_wasm_v2_packet(node_id: u8, module_id: u8, events: &[WasmEvent]) -> Vec<u8> {
    let mut out = vec![0u8; 8 + events.len() * 5];
    out[0..4].copy_from_slice(&MAGIC_WASM_V2.to_le_bytes());
    out[4] = node_id;
    out[5] = module_id;
    out[6..8].copy_from_slice(&(events.len() as u16).to_le_bytes());
    let mut off = 8usize;
    for event in events {
        out[off] = event.event_type;
        out[off + 1..off + 5].copy_from_slice(&event.value.to_le_bytes());
        off += 5;
    }
    out
}
