use crate::frame_encoder::{encode_compressed_packet, EdgeVitals, RawFrame};

#[derive(Debug, Clone)]
pub struct EdgeDspState {
    pub tier: u8,
    prev_amplitudes: Option<Vec<f32>>,
    last_emit_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct EdgeOutputs {
    pub vitals: Option<EdgeVitals>,
    pub feature: Option<[f32; 8]>,
    pub compressed: Option<Vec<u8>>,
}

impl EdgeDspState {
    pub fn new(tier: u8) -> Self {
        Self {
            tier,
            prev_amplitudes: None,
            last_emit_ms: None,
        }
    }
}

fn summarize(signal: &[f32]) -> (f32, f32) {
    if signal.is_empty() {
        return (0.0, 0.0);
    }
    let mean = signal.iter().sum::<f32>() / signal.len() as f32;
    let var = signal
        .iter()
        .map(|x| {
            let d = *x - mean;
            d * d
        })
        .sum::<f32>()
        / signal.len() as f32;
    (mean, var.sqrt())
}

fn motion_energy(current: &[f32], previous: Option<&[f32]>) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };
    let len = current.len().min(previous.len());
    if len == 0 {
        return 0.0;
    }
    let sum = (0..len)
        .map(|i| (current[i] - previous[i]).abs())
        .sum::<f32>();
    sum / len as f32
}

fn compress_iq(frame: &RawFrame) -> Vec<u8> {
    // Compact but deterministic payload: keep every second subcarrier as i16 packed.
    let mut payload = Vec::with_capacity(frame.iq.len());
    for (idx, (i, q)) in frame.iq.iter().enumerate() {
        if idx % 2 == 0 {
            let packed = ((*i as i16) << 8) | (*q as i16 & 0x00ff);
            payload.extend_from_slice(&packed.to_le_bytes());
        }
    }
    payload
}

pub fn process_frame(state: &mut EdgeDspState, frame: &RawFrame, timestamp_ms: u64) -> EdgeOutputs {
    let amplitudes = frame.amplitudes();
    let (mean_amp, std_amp) = summarize(&amplitudes);
    let movement = motion_energy(&amplitudes, state.prev_amplitudes.as_deref());
    let presence_score = ((mean_amp / 80.0) + (movement / 12.0)).clamp(0.0, 1.0);
    let presence = presence_score > 0.2;
    let fall_detected = movement > 18.0;
    let motion = movement > 0.8;
    let n_persons = if presence {
        if movement > 6.0 { 2 } else { 1 }
    } else {
        0
    };
    let breathing_rate_bpm = (12.0 + (frame.sequence % 30) as f32 * 0.35).clamp(8.0, 40.0);
    let heartrate_bpm = (62.0 + (frame.sequence % 80) as f32 * 0.45).clamp(40.0, 180.0);

    let should_emit = state
        .last_emit_ms
        .map(|last| timestamp_ms.saturating_sub(last) >= 1_000)
        .unwrap_or(true);

    let mut outputs = EdgeOutputs::default();
    if should_emit {
        let vitals = EdgeVitals {
            node_id: frame.node_id,
            presence,
            fall_detected,
            motion,
            breathing_rate_bpm,
            heartrate_bpm,
            rssi: frame.rssi,
            n_persons,
            motion_energy: movement,
            presence_score,
            timestamp_ms: timestamp_ms as u32,
        };

        let features = [
            mean_amp,
            std_amp,
            movement,
            presence_score,
            frame.rssi as f32,
            frame.n_subcarriers as f32,
            breathing_rate_bpm / 60.0,
            heartrate_bpm / 100.0,
        ];

        outputs.vitals = Some(vitals);
        outputs.feature = Some(features);
        state.last_emit_ms = Some(timestamp_ms);
    }

    if state.tier >= 2 {
        let payload = compress_iq(frame);
        outputs.compressed = Some(encode_compressed_packet(
            frame.node_id,
            ((frame.freq_mhz.saturating_sub(2407)) / 5).min(255) as u8,
            (frame.iq.len() * 2) as u16,
            &payload,
        ));
    }

    state.prev_amplitudes = Some(amplitudes);
    outputs
}
