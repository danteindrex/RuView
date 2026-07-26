//! Tiny CSI-embedding model inference.
//!
//! Runs the `ruvnet/wifi-densepose-pretrained` encoder on-device / on-hub:
//!
//! ```text
//! x[8] ─Linear(8→64)─ BN1 ─ReLU─ Linear(64→128) ─ BN2─► e[128]
//! e' = e + scaling·((e·A)·B)          # per-room LoRA (node-N.json, rank r)
//! p  = sigmoid(e'·w_head + b_head)     # presence-head.json
//! ```
//!
//! The 8 input features are, in order (authoritative — `scripts/deep-scan.js`):
//! `[Presence, Motion, Breathing, HeartRate, PhaseVar, Persons, Fall, RSSI]`.
//!
//! This crate is the single source of truth for the math: the sensing-server
//! (hub) and the Raspberry Pi node agent both call it, and the ESP32 C port is
//! numerically parity-tested against it.

use serde::Deserialize;

/// Number of model input features.
pub const N_FEATURES: usize = 8;
/// Hidden width.
pub const HIDDEN: usize = 64;
/// Embedding width.
pub const EMBED: usize = 128;

/// Canonical input-feature order (from `scripts/deep-scan.js`). The caller must
/// assemble the input vector in exactly this order.
pub const FEATURE_NAMES: [&str; N_FEATURES] = [
    "presence", "motion", "breathing", "heart_rate", "phase_var", "persons", "fall", "rssi",
];

/// A dense layer: `y = W·x + b`, `weight` is row-major `[out * in]`.
#[derive(Debug, Clone)]
pub struct Linear {
    pub weight: Vec<f32>,
    pub bias: Vec<f32>,
    pub in_dim: usize,
    pub out_dim: usize,
}

impl Linear {
    fn forward(&self, x: &[f32], out: &mut [f32]) {
        debug_assert_eq!(x.len(), self.in_dim);
        debug_assert_eq!(out.len(), self.out_dim);
        for o in 0..self.out_dim {
            let row = o * self.in_dim;
            let mut s = self.bias[o];
            for i in 0..self.in_dim {
                s += self.weight[row + i] * x[i];
            }
            out[o] = s;
        }
    }
}

/// Inference-mode BatchNorm1d: `y = gamma·(x-mean)/sqrt(var+eps) + beta`.
#[derive(Debug, Clone)]
pub struct BatchNorm {
    pub gamma: Vec<f32>,
    pub beta: Vec<f32>,
    pub mean: Vec<f32>,
    pub var: Vec<f32>,
    pub eps: f32,
}

impl BatchNorm {
    fn apply(&self, x: &mut [f32]) {
        for i in 0..x.len() {
            x[i] = self.gamma[i] * (x[i] - self.mean[i]) / (self.var[i] + self.eps).sqrt()
                + self.beta[i];
        }
    }
}

/// Rank-`r` LoRA adapter over the embedding: `e' = e + scaling·((e·A)·B)`,
/// `A` is `[dim × r]`, `B` is `[r × dim]` (both row-major).
#[derive(Debug, Clone)]
pub struct LoraAdapter {
    pub a: Vec<f32>, // dim * rank
    pub b: Vec<f32>, // rank * dim
    pub rank: usize,
    pub dim: usize,
    pub scaling: f32,
}

impl LoraAdapter {
    fn apply(&self, e: &mut [f32]) {
        debug_assert_eq!(e.len(), self.dim);
        // t = e·A  (length rank)
        let mut t = vec![0.0f32; self.rank];
        for r in 0..self.rank {
            let mut s = 0.0f32;
            for i in 0..self.dim {
                s += e[i] * self.a[i * self.rank + r];
            }
            t[r] = s;
        }
        // e += scaling · (t·B)
        for o in 0..self.dim {
            let mut s = 0.0f32;
            for r in 0..self.rank {
                s += t[r] * self.b[r * self.dim + o];
            }
            e[o] += self.scaling * s;
        }
    }
}

/// Linear presence head: `p = sigmoid(e·w + b)`.
#[derive(Debug, Clone)]
pub struct PresenceHead {
    pub weight: Vec<f32>,
    pub bias: f32,
}

impl PresenceHead {
    fn presence(&self, e: &[f32]) -> f32 {
        let mut s = self.bias;
        for i in 0..e.len() {
            s += self.weight[i] * e[i];
        }
        1.0 / (1.0 + (-s).exp())
    }
}

/// Result of one inference.
#[derive(Debug, Clone)]
pub struct Inference {
    /// 128-d embedding (post room-adaptation).
    pub embedding: Vec<f32>,
    /// Presence probability in `[0, 1]`.
    pub presence: f32,
}

/// The full on-device model: encoder + optional per-room LoRA + presence head.
#[derive(Debug, Clone)]
pub struct EdgeModel {
    pub w1: Linear,
    pub bn1: BatchNorm,
    pub w2: Linear,
    pub bn2: BatchNorm,
    pub lora: Option<LoraAdapter>,
    pub head: PresenceHead,
}

impl EdgeModel {
    /// Run the forward pass on the 8-feature input (in [`FEATURE_NAMES`] order).
    pub fn infer(&self, x: &[f32; N_FEATURES]) -> Inference {
        let mut h = vec![0.0f32; HIDDEN];
        self.w1.forward(x, &mut h);
        self.bn1.apply(&mut h);
        for v in h.iter_mut() {
            if *v < 0.0 {
                *v = 0.0; // ReLU
            }
        }
        let mut e = vec![0.0f32; EMBED];
        self.w2.forward(&h, &mut e);
        self.bn2.apply(&mut e);
        if let Some(lora) = &self.lora {
            lora.apply(&mut e);
        }
        let presence = self.head.presence(&e);
        Inference { embedding: e, presence }
    }

    /// Attach (or replace) the per-room LoRA adapter.
    pub fn with_lora(mut self, lora: Option<LoraAdapter>) -> Self {
        self.lora = lora;
        self
    }

    /// Build from the bundled `csi-embed-v2.safetensors` bytes + presence-head JSON.
    pub fn load(safetensors: &[u8], presence_head_json: &str) -> Result<Self, String> {
        let st = SafeTensors::parse(safetensors)?;
        let bn = |p: &str| -> Result<BatchNorm, String> {
            Ok(BatchNorm {
                gamma: st.f32(&format!("{p}.weight"))?,
                beta: st.f32(&format!("{p}.bias"))?,
                mean: st.f32(&format!("{p}.running_mean"))?,
                var: st.f32(&format!("{p}.running_var"))?,
                eps: 1e-5,
            })
        };
        let lin = |p: &str, out_dim: usize, in_dim: usize| -> Result<Linear, String> {
            Ok(Linear {
                weight: st.f32(&format!("{p}.weight"))?,
                bias: st.f32(&format!("{p}.bias"))?,
                in_dim,
                out_dim,
            })
        };
        Ok(EdgeModel {
            w1: lin("w1", HIDDEN, N_FEATURES)?,
            bn1: bn("bn1")?,
            w2: lin("w2", EMBED, HIDDEN)?,
            bn2: bn("bn2")?,
            lora: None,
            head: PresenceHead::from_json(presence_head_json)?,
        })
    }
}

impl PresenceHead {
    /// Parse `presence-head.json` (`{"weights":[..128..],"bias":..}`).
    pub fn from_json(json: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Raw {
            weights: Vec<f32>,
            bias: f32,
        }
        let r: Raw = serde_json::from_str(json).map_err(|e| format!("presence-head json: {e}"))?;
        if r.weights.len() != EMBED {
            return Err(format!("presence head expects {EMBED} weights, got {}", r.weights.len()));
        }
        Ok(PresenceHead { weight: r.weights, bias: r.bias })
    }
}

impl LoraAdapter {
    /// Parse a `node-N.json` room adapter (`{weights:{loraA:[[..]],loraB:[[..]],scaling},inputDim,outputDim,config:{rank}}`).
    pub fn from_json(json: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Weights {
            #[serde(rename = "loraA")]
            lora_a: Vec<Vec<f32>>,
            #[serde(rename = "loraB")]
            lora_b: Vec<Vec<f32>>,
            scaling: f32,
        }
        #[derive(Deserialize)]
        struct Raw {
            weights: Weights,
            #[serde(rename = "inputDim")]
            input_dim: usize,
        }
        let r: Raw = serde_json::from_str(json).map_err(|e| format!("lora json: {e}"))?;
        let dim = r.input_dim;
        let rank = r.weights.lora_a.first().map(|row| row.len()).unwrap_or(0);
        if r.weights.lora_a.len() != dim || r.weights.lora_b.len() != rank {
            return Err(format!(
                "lora shape mismatch: A={}x{}, B={}x{}, dim={dim} rank={rank}",
                r.weights.lora_a.len(),
                rank,
                r.weights.lora_b.len(),
                r.weights.lora_b.first().map(|x| x.len()).unwrap_or(0)
            ));
        }
        let a: Vec<f32> = r.weights.lora_a.into_iter().flatten().collect();
        let b: Vec<f32> = r.weights.lora_b.into_iter().flatten().collect();
        Ok(LoraAdapter { a, b, rank, dim, scaling: r.weights.scaling })
    }
}

/// Minimal safetensors reader (F32 tensors only): `u64 header_len | JSON | data`.
struct SafeTensors<'a> {
    header: serde_json::Value,
    data: &'a [u8],
}

impl<'a> SafeTensors<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < 8 {
            return Err("safetensors too short".into());
        }
        let n = u64::from_le_bytes(bytes[0..8].try_into().unwrap()) as usize;
        let end = 8 + n;
        if bytes.len() < end {
            return Err("safetensors header out of range".into());
        }
        let header: serde_json::Value =
            serde_json::from_slice(&bytes[8..end]).map_err(|e| format!("safetensors header: {e}"))?;
        Ok(SafeTensors { header, data: &bytes[end..] })
    }

    fn f32(&self, name: &str) -> Result<Vec<f32>, String> {
        let t = self.header.get(name).ok_or_else(|| format!("tensor '{name}' missing"))?;
        let dtype = t.get("dtype").and_then(|v| v.as_str()).unwrap_or("");
        if dtype != "F32" {
            return Err(format!("tensor '{name}' dtype {dtype} != F32"));
        }
        let offs = t
            .get("data_offsets")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("tensor '{name}' has no data_offsets"))?;
        let start = offs[0].as_u64().unwrap() as usize;
        let stop = offs[1].as_u64().unwrap() as usize;
        let slice = &self.data[start..stop];
        Ok(slice.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Bundled model files (committed under the desktop crate's resources).
    const CSI_EMBED_V2: &[u8] =
        include_bytes!("../../wifi-densepose-desktop/resources/models/csi-embed-v2.safetensors");
    const PRESENCE_HEAD: &str =
        include_str!("../../wifi-densepose-desktop/resources/models/presence-head.json");
    const NODE1_LORA: &str =
        include_str!("../../wifi-densepose-desktop/resources/models/node-1.json");

    #[test]
    fn hermetic_forward_math() {
        // Identity-ish tiny model: w1 = ones-diagonal-ish, BN pass-through.
        let w1 = Linear { weight: vec![0.0; HIDDEN * N_FEATURES], bias: vec![1.0; HIDDEN], in_dim: N_FEATURES, out_dim: HIDDEN };
        let passthrough = |n: usize| BatchNorm { gamma: vec![1.0; n], beta: vec![0.0; n], mean: vec![0.0; n], var: vec![1.0 - 1e-5; n], eps: 1e-5 };
        let w2 = Linear { weight: vec![0.0; EMBED * HIDDEN], bias: vec![0.5; EMBED], in_dim: HIDDEN, out_dim: EMBED };
        let head = PresenceHead { weight: vec![0.0; EMBED], bias: 0.0 };
        let m = EdgeModel { w1, bn1: passthrough(HIDDEN), w2, bn2: passthrough(EMBED), lora: None, head };
        let out = m.infer(&[0.0; N_FEATURES]);
        // h = ReLU(BN(1.0)) = 1.0 ; e = 0*h + 0.5 = 0.5 ; presence = sigmoid(0) = 0.5
        assert!((out.embedding[0] - 0.5).abs() < 1e-4, "embed {}", out.embedding[0]);
        assert!((out.presence - 0.5).abs() < 1e-4, "presence {}", out.presence);
    }

    #[test]
    fn loads_real_bundled_weights() {
        let model = EdgeModel::load(CSI_EMBED_V2, PRESENCE_HEAD).expect("load model");
        assert_eq!(model.w1.in_dim, N_FEATURES);
        assert_eq!(model.w2.out_dim, EMBED);
        // A plausible "someone present" feature vector.
        let x = [0.9, 0.6, 14.0, 68.0, 0.4, 1.0, 0.0, -55.0];
        let out = model.infer(&x);
        assert_eq!(out.embedding.len(), EMBED);
        assert!(out.presence.is_finite() && (0.0..=1.0).contains(&out.presence), "p={}", out.presence);
        assert!(out.embedding.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn per_room_lora_changes_output() {
        let base = EdgeModel::load(CSI_EMBED_V2, PRESENCE_HEAD).expect("load");
        let lora = LoraAdapter::from_json(NODE1_LORA).expect("lora");
        assert_eq!(lora.dim, EMBED);
        let adapted = base.clone().with_lora(Some(lora));
        let x = [0.5, 0.3, 12.0, 60.0, 0.2, 1.0, 0.0, -60.0];
        let a = base.infer(&x);
        let b = adapted.infer(&x);
        // The room adapter must actually move the embedding.
        let diff: f32 = a.embedding.iter().zip(&b.embedding).map(|(x, y)| (x - y).abs()).sum();
        assert!(diff > 1e-6, "LoRA had no effect (diff={diff})");
    }
}
