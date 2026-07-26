//! On-device neural inference for the Raspberry Pi node agent.
//!
//! Reuses the shared `wifi-densepose-edge-infer` crate (identical math to the
//! hub), so the Pi runs the CSI-embedding model locally and reports the result
//! upstream in an inference packet. The model files are embedded (tiny) so the
//! agent is self-contained.

use std::collections::HashMap;
use std::sync::OnceLock;

use wifi_densepose_edge_infer::{EdgeModel, LoraAdapter, N_FEATURES};

use crate::frame_encoder::EdgeVitals;

const CSI_EMBED_V2: &[u8] =
    include_bytes!("../../wifi-densepose-desktop/resources/models/csi-embed-v2.safetensors");
const PRESENCE_HEAD: &str =
    include_str!("../../wifi-densepose-desktop/resources/models/presence-head.json");
const NODE1_LORA: &str = include_str!("../../wifi-densepose-desktop/resources/models/node-1.json");
const NODE2_LORA: &str = include_str!("../../wifi-densepose-desktop/resources/models/node-2.json");

struct Models {
    base: EdgeModel,
    per_node: HashMap<u8, EdgeModel>,
}

static MODELS: OnceLock<Option<Models>> = OnceLock::new();

fn models() -> Option<&'static Models> {
    MODELS
        .get_or_init(|| {
            let base = EdgeModel::load(CSI_EMBED_V2, PRESENCE_HEAD).ok()?;
            let mut per_node = HashMap::new();
            for (id, json) in [(1u8, NODE1_LORA), (2u8, NODE2_LORA)] {
                if let Ok(lora) = LoraAdapter::from_json(json) {
                    per_node.insert(id, base.clone().with_lora(Some(lora)));
                }
            }
            Some(Models { base, per_node })
        })
        .as_ref()
}

/// Assemble the model's canonical 8-vector `[presence, motion, breathing,
/// heart_rate, phase_var, persons, fall, rssi]` from the edge DSP vitals.
///
/// NOTE: the Pi edge DSP has no phase-variance feature, so `presence_score` is
/// used as a stand-in for that slot (documented placeholder — matches the hub's
/// best-effort assembly). The Pi DSP itself is a lightweight approximation.
pub fn features_from_vitals(v: &EdgeVitals) -> [f32; N_FEATURES] {
    [
        if v.presence { 1.0 } else { 0.0 },
        v.motion_energy,
        v.breathing_rate_bpm,
        v.heartrate_bpm,
        v.presence_score,
        v.n_persons as f32,
        if v.fall_detected { 1.0 } else { 0.0 },
        v.rssi as f32,
    ]
}

/// Result of on-device inference.
pub struct NodeInference {
    pub presence: f32,
    pub adapted: bool,
}

/// Run the model for `node_id` on the assembled features. `None` if the model
/// could not be loaded (agent then simply skips the inference packet).
pub fn infer(node_id: u8, features: [f32; N_FEATURES]) -> Option<NodeInference> {
    let m = models()?;
    let (model, adapted) = match m.per_node.get(&node_id) {
        Some(model) => (model, true),
        None => (&m.base, false),
    };
    Some(NodeInference { presence: model.infer(&features).presence, adapted })
}
