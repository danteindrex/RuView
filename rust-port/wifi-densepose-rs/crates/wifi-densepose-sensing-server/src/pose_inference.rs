//! Live pose inference from trained RVF linear models.
//!
//! Loads a trained linear pose model (weights + feature-normalization stats)
//! from an `.rvf` container and applies it per-frame to CSI subcarrier
//! amplitudes to produce 17 COCO keypoints.
//!
//! The feature extraction here is a faithful copy of the extractor used at
//! training time (`training_api.rs`, currently not compiled into the binary):
//! raw amplitudes, per-subcarrier window variance, temporal gradients,
//! Goertzel band powers, and global scalars — in that exact order. Any change
//! to this ordering or math invalidates existing trained models.

use std::collections::VecDeque;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::rvf_container::RvfReader;

/// Number of COCO keypoints.
pub const N_KEYPOINTS: usize = 17;
/// Dimensions per keypoint in the target vector (x, y, z).
const DIMS_PER_KP: usize = 3;
/// Total target dimensionality: 17 * 3 = 51.
const N_TARGETS: usize = N_KEYPOINTS * DIMS_PER_KP;
/// Sliding window depth for variance features (must match training).
const VARIANCE_WINDOW: usize = 10;
/// Goertzel frequency bands in Hz (must match training).
const FREQ_BANDS: [f64; 9] = [0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 1.0, 2.0, 3.0];
/// Global scalar features appended at the end (mean, std, motion score).
const N_GLOBAL_FEATURES: usize = 3;

/// Feature normalization statistics stored in the RVF metadata at training
/// time. Field layout must match the trainer's `FeatureStats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureStats {
    pub mean: Vec<f64>,
    pub std: Vec<f64>,
    pub n_features: usize,
    pub n_subcarriers: usize,
}

/// A trained pose model loaded into memory, ready for per-frame inference.
#[derive(Debug, Clone)]
pub struct LoadedPoseModel {
    pub id: String,
    pub weights: Vec<f32>,
    pub stats: FeatureStats,
}

/// Total number of features produced for a given subcarrier count.
fn feature_dim(n_sub: usize) -> usize {
    n_sub + n_sub + n_sub + FREQ_BANDS.len() + N_GLOBAL_FEATURES
}

/// Load a trained linear pose model from an RVF container.
///
/// Requires the SEG_VEC weights segment and `feature_stats` +
/// `model_config.type == "linear"` in the SEG_META metadata.
pub fn load_pose_model_from_rvf(path: &Path) -> Result<LoadedPoseModel, String> {
    let reader =
        RvfReader::from_file(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let weights = reader
        .weights()
        .ok_or_else(|| "RVF has no weights segment".to_string())?;
    let meta = reader
        .metadata()
        .ok_or_else(|| "RVF has no metadata segment".to_string())?;

    // Older trainer versions wrote no model_config; those are linear models.
    // Only reject an explicit non-linear type.
    if let Some(model_type) = meta.pointer("/model_config/type").and_then(|v| v.as_str()) {
        if model_type != "linear" {
            return Err(format!(
                "unsupported model type '{model_type}' (expected 'linear')"
            ));
        }
    }

    let stats: FeatureStats = meta
        .get("feature_stats")
        .cloned()
        .ok_or_else(|| "RVF metadata missing feature_stats".to_string())
        .and_then(|v| {
            serde_json::from_value(v).map_err(|e| format!("feature_stats parse: {e}"))
        })?;

    let expected = N_TARGETS * stats.n_features + N_TARGETS;
    if weights.len() < expected {
        return Err(format!(
            "weights segment too short: {} < {expected} expected for {} features",
            weights.len(),
            stats.n_features
        ));
    }

    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    Ok(LoadedPoseModel { id, weights, stats })
}

/// Goertzel algorithm: power at a normalised frequency from a signal buffer.
fn goertzel_power(signal: &[f64], freq_norm: f64) -> f64 {
    let n = signal.len();
    if n == 0 {
        return 0.0;
    }
    let coeff = 2.0 * (2.0 * std::f64::consts::PI * freq_norm).cos();
    let mut s0 = 0.0f64;
    let mut s1 = 0.0f64;
    let mut s2;
    for &x in signal {
        s2 = s1;
        s1 = s0;
        s0 = x + coeff * s1 - s2;
    }
    let power = s0 * s0 + s1 * s1 - coeff * s0 * s1;
    (power / (n as f64)).max(0.0)
}

/// Extract the training-time feature vector from raw subcarrier amplitudes.
///
/// `window` is the most recent frames oldest-first (like the trainer's
/// sliding window); `prev` is the immediately preceding frame.
fn extract_features(
    current: &[f64],
    window: &[&[f64]],
    prev: Option<&[f64]>,
    sample_rate_hz: f64,
) -> Vec<f64> {
    let n_sub = current.len().max(1);
    let mut features = Vec::with_capacity(feature_dim(n_sub));

    // 1. Raw subcarrier amplitudes.
    features.extend_from_slice(current);
    while features.len() < n_sub {
        features.push(0.0);
    }

    // 2. Per-subcarrier variance over the sliding window.
    for k in 0..n_sub {
        if window.is_empty() {
            features.push(0.0);
            continue;
        }
        let n = window.len() as f64;
        let mut sum = 0.0f64;
        let mut sq_sum = 0.0f64;
        for w in window {
            let a = if k < w.len() { w[k] } else { 0.0 };
            sum += a;
            sq_sum += a * a;
        }
        let mean = sum / n;
        let var = (sq_sum / n - mean * mean).max(0.0);
        features.push(var);
    }

    // 3. Temporal gradient vs previous frame.
    for k in 0..n_sub {
        let grad = match prev {
            Some(p) => {
                let cur = if k < current.len() { current[k] } else { 0.0 };
                let prv = if k < p.len() { p[k] } else { 0.0 };
                (cur - prv).abs()
            }
            None => 0.0,
        };
        features.push(grad);
    }

    // 4. Goertzel power at key frequency bands over the window mean series.
    let ts: Vec<f64> = window
        .iter()
        .map(|w| {
            let n = w.len().max(1) as f64;
            w.iter().sum::<f64>() / n
        })
        .collect();
    for &freq_hz in &FREQ_BANDS {
        let freq_norm = if sample_rate_hz > 0.0 {
            freq_hz / sample_rate_hz
        } else {
            0.0
        };
        features.push(goertzel_power(&ts, freq_norm));
    }

    // 5. Global scalars: mean amplitude, std amplitude, motion score.
    let mean_amp = if current.is_empty() {
        0.0
    } else {
        current.iter().sum::<f64>() / current.len() as f64
    };
    let std_amp = if current.len() > 1 {
        let var = current
            .iter()
            .map(|a| (a - mean_amp).powi(2))
            .sum::<f64>()
            / (current.len() - 1) as f64;
        var.sqrt()
    } else {
        0.0
    };
    let motion_score = match prev {
        Some(p) => {
            let n_cmp = n_sub.min(p.len());
            if n_cmp > 0 {
                let diff: f64 = (0..n_cmp)
                    .map(|k| {
                        let c = if k < current.len() { current[k] } else { 0.0 };
                        let pv = if k < p.len() { p[k] } else { 0.0 };
                        (c - pv).powi(2)
                    })
                    .sum::<f64>()
                    / n_cmp as f64;
                (diff / (mean_amp * mean_amp + 1e-9)).sqrt().clamp(0.0, 1.0)
            } else {
                0.0
            }
        }
        None => 0.0,
    };
    features.push(mean_amp);
    features.push(std_amp);
    features.push(motion_score);

    features
}

/// Run the trained linear model on the current frame.
///
/// Returns 17 keypoints as `[x, y, z, confidence]`, or None when the input
/// subcarrier count does not match what the model was trained on (running
/// anyway would produce garbage rather than fail loudly).
pub fn infer_pose(
    model: &LoadedPoseModel,
    frame_history: &VecDeque<Vec<f64>>,
    sample_rate_hz: f64,
) -> Option<Vec<[f64; 4]>> {
    let current = frame_history.back()?;
    if current.len() != model.stats.n_subcarriers {
        // Log at warn once per mismatch burst would need state; keep it debug-quiet.
        warn!(
            "pose inference skipped: frame has {} subcarriers, model expects {}",
            current.len(),
            model.stats.n_subcarriers
        );
        return None;
    }

    let n_feat = model.stats.n_features;

    // Window = most recent VARIANCE_WINDOW frames, oldest-first.
    let window: Vec<&[f64]> = frame_history
        .iter()
        .rev()
        .take(VARIANCE_WINDOW)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|v| v.as_slice())
        .collect();
    let prev = frame_history
        .iter()
        .rev()
        .nth(1)
        .map(|v| v.as_slice());

    let mut features = extract_features(current, &window, prev, sample_rate_hz);

    // Normalize with training-time stats.
    for (j, val) in features.iter_mut().enumerate() {
        if j < n_feat {
            let m = model.stats.mean.get(j).copied().unwrap_or(0.0);
            let s = model.stats.std.get(j).copied().unwrap_or(1.0);
            *val = (*val - m) / s;
        }
    }
    features.resize(n_feat, 0.0);

    // Linear head: output[t] = W[t] . x + bias[t].
    let weights_end = N_TARGETS * n_feat;
    let mut keypoints = Vec::with_capacity(N_KEYPOINTS);
    for k in 0..N_KEYPOINTS {
        let mut coords = [0.0f64; 4];
        for d in 0..DIMS_PER_KP {
            let t = k * DIMS_PER_KP + d;
            let row_start = t * n_feat;
            let mut sum = model
                .weights
                .get(weights_end + t)
                .map(|&b| b as f64)
                .unwrap_or(0.0);
            for (j, feat) in features.iter().enumerate() {
                let w = model
                    .weights
                    .get(row_start + j)
                    .map(|&v| v as f64)
                    .unwrap_or(0.0);
                sum += w * feat;
            }
            coords[d] = sum;
        }
        let feat_magnitude: f64 =
            features.iter().map(|v| v.abs()).sum::<f64>() / features.len().max(1) as f64;
        coords[3] = (1.0 / (1.0 + (-feat_magnitude + 1.0).exp())).clamp(0.1, 0.99);
        keypoints.push(coords);
    }

    Some(keypoints)
}
