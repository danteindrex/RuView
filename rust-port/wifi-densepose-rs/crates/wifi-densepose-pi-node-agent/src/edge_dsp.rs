use std::collections::VecDeque;

use crate::frame_encoder::{encode_compressed_packet, EdgeVitals, RawFrame};

/// Seconds of amplitude history retained for vital-sign estimation. Breathing
/// (down to 0.1 Hz) needs a long window; 30 s covers ~3 cycles at the low end.
const VITALS_WINDOW_S: f64 = 30.0;
/// Physiological bands (Hz): breathing 6–30 BPM, heart 40–180 BPM.
const BR_LO_HZ: f64 = 0.1;
const BR_HI_HZ: f64 = 0.5;
const HR_LO_HZ: f64 = 0.7;
const HR_HI_HZ: f64 = 3.0;

#[derive(Debug, Clone)]
pub struct EdgeDspState {
    pub tier: u8,
    prev_amplitudes: Option<Vec<f32>>,
    last_emit_ms: Option<u64>,
    /// (timestamp_ms, mean-amplitude) history for real vital-sign estimation.
    /// Breathing/heartbeat modulate CSI amplitude; we recover the rate from the
    /// periodicity of this series — never from a frame counter.
    amp_hist: VecDeque<(u64, f32)>,
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
            amp_hist: VecDeque::new(),
        }
    }
}

/// Estimate a rate (BPM) from an evenly-ish sampled `(t_ms, value)` series by
/// autocorrelation, searching only the lag range for the given frequency band.
/// Returns `Some(bpm)` when a clear periodic peak exists, else `None` — we never
/// invent a value when the signal doesn't support one.
fn estimate_rate_bpm(hist: &VecDeque<(u64, f32)>, lo_hz: f64, hi_hz: f64) -> Option<f32> {
    let n = hist.len();
    if n < 32 {
        return None;
    }
    let t0 = hist.front()?.0;
    let t1 = hist.back()?.0;
    let span_s = (t1.saturating_sub(t0)) as f64 / 1000.0;
    if span_s < 1.0 / lo_hz {
        return None; // not enough time to see the slowest cycle in the band
    }
    let fs = (n as f64 - 1.0) / span_s; // mean sample rate (Hz)
    if !fs.is_finite() || fs <= 0.0 {
        return None;
    }
    // Detrend (remove DC + slow drift via mean subtraction).
    let mean = hist.iter().map(|(_, v)| *v as f64).sum::<f64>() / n as f64;
    let x: Vec<f64> = hist.iter().map(|(_, v)| *v as f64 - mean).collect();
    let energy: f64 = x.iter().map(|v| v * v).sum();
    if energy < 1e-9 {
        return None; // flat signal — no vitals present
    }
    // Lag range (samples) corresponding to the frequency band.
    let min_lag = ((fs / hi_hz).floor() as usize).max(1);
    let max_lag = ((fs / lo_hz).ceil() as usize).min(n / 2);
    if min_lag >= max_lag {
        return None;
    }
    let (mut best_lag, mut best_corr) = (0usize, 0.0f64);
    for lag in min_lag..=max_lag {
        let mut c = 0.0;
        for i in 0..(n - lag) {
            c += x[i] * x[i + lag];
        }
        let norm = c / energy;
        if norm > best_corr {
            best_corr = norm;
            best_lag = lag;
        }
    }
    // Require a meaningful peak; otherwise report "no measurable rate".
    if best_lag == 0 || best_corr < 0.3 {
        return None;
    }
    let period_s = best_lag as f64 / fs;
    Some((60.0 / period_s) as f32)
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

/// Compact deterministic codec: keep every second (even-indexed) subcarrier,
/// emitting its raw I byte then Q byte (`i8` two's complement), so signs
/// survive the round trip. Inverse is [`decompress_iq`].
fn compress_iq(frame: &RawFrame) -> Vec<u8> {
    // 2 bytes per kept (even-indexed) subcarrier.
    let mut payload = Vec::with_capacity(frame.iq.len().div_ceil(2) * 2);
    for (idx, (i, q)) in frame.iq.iter().enumerate() {
        if idx % 2 == 0 {
            payload.push(*i as u8);
            payload.push(*q as u8);
        }
    }
    payload
}

/// Inverse of `compress_iq`: reconstruct the kept (even-indexed) subcarriers
/// from a compressed payload (any trailing odd byte is ignored).
pub fn decompress_iq(payload: &[u8]) -> Vec<(i8, i8)> {
    payload
        .chunks_exact(2)
        .map(|pair| (pair[0] as i8, pair[1] as i8))
        .collect()
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

    // Push this frame's mean amplitude into the vitals history and trim to the
    // window, then estimate real breathing/heart rates from the signal's
    // periodicity. 0.0 means "not measurable yet" (never a fabricated number);
    // the hub's VitalSignDetector remains the authoritative vitals source.
    state.amp_hist.push_back((timestamp_ms, mean_amp));
    while let Some(&(t, _)) = state.amp_hist.front() {
        if (timestamp_ms.saturating_sub(t)) as f64 / 1000.0 > VITALS_WINDOW_S {
            state.amp_hist.pop_front();
        } else {
            break;
        }
    }
    // Only report vitals when a person is present (no target ⇒ no vitals).
    let (breathing_rate_bpm, heartrate_bpm) = if presence {
        (
            estimate_rate_bpm(&state.amp_hist, BR_LO_HZ, BR_HI_HZ).unwrap_or(0.0),
            estimate_rate_bpm(&state.amp_hist, HR_LO_HZ, HR_HI_HZ).unwrap_or(0.0),
        )
    } else {
        (0.0, 0.0)
    };

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

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_with_iq(iq: Vec<(i8, i8)>) -> RawFrame {
        RawFrame {
            node_id: 10,
            n_antennas: 1,
            n_subcarriers: iq.len() as u16,
            freq_mhz: 2437,
            sequence: 7,
            rssi: -48,
            noise_floor: -92,
            iq,
        }
    }

    #[test]
    fn compress_iq_round_trips_signed_values() {
        let iq = vec![(-128, 127), (5, -5), (-1, 0), (64, -64), (100, -100)];
        let frame = frame_with_iq(iq.clone());
        let decoded = decompress_iq(&compress_iq(&frame));
        let kept: Vec<(i8, i8)> = iq.iter().copied().step_by(2).collect();
        assert_eq!(decoded, kept);
    }

    #[test]
    fn estimate_rate_recovers_known_breathing_frequency() {
        // 15 BPM (0.25 Hz) sinusoid sampled at 20 Hz for 30 s.
        let fs = 20.0;
        let f = 0.25;
        let mut hist = VecDeque::new();
        for i in 0..(fs as u64 * 30) {
            let t_ms = (i as f64 / fs * 1000.0) as u64;
            let v = (2.0 * std::f64::consts::PI * f * (i as f64 / fs)).sin() as f32;
            hist.push_back((t_ms, v));
        }
        let bpm = estimate_rate_bpm(&hist, BR_LO_HZ, BR_HI_HZ).expect("should find the rate");
        assert!((bpm - 15.0).abs() < 1.5, "estimated {bpm} BPM, expected ~15");
    }

    #[test]
    fn estimate_rate_returns_none_for_flat_signal() {
        // A flat (no-oscillation) signal must NOT invent a vital sign.
        let mut hist = VecDeque::new();
        for i in 0..600 {
            hist.push_back((i * 50, 5.0));
        }
        assert!(estimate_rate_bpm(&hist, BR_LO_HZ, BR_HI_HZ).is_none());
    }

    #[test]
    fn tier2_compressed_packet_payload_is_recoverable() {
        let mut state = EdgeDspState::new(2);
        let frame = frame_with_iq(vec![(-100, 50), (25, -25), (-7, 7), (0, -128)]);
        let outputs = process_frame(&mut state, &frame, 1_000);
        let packet = outputs
            .compressed
            .expect("tier >= 2 must emit a compressed packet");
        // Skip the 10-byte header written by encode_compressed_packet.
        let decoded = decompress_iq(&packet[10..]);
        assert_eq!(decoded, vec![(-100, 50), (-7, 7)]);
    }
}
