//! Hub-side neural inference: runs the bundled CSI-embedding model per node.
//!
//! The model files are embedded (they are tiny — <100 KB total) so the server
//! always has them, independent of the desktop resource dir. The base encoder is
//! shared; per-room LoRA adapters (`node-N.json`) are selected by node id.
//!
//! This is the **reference** implementation (Phase 1 of the on-device inference
//! plan). The Pi agent reuses the same `wifi-densepose-edge-infer` crate, and the
//! ESP32 C port is numerically checked against it.

use std::collections::HashMap;
use std::sync::OnceLock;

use wifi_densepose_edge_infer::{EdgeModel, Inference, LoraAdapter, FEATURE_NAMES, N_FEATURES};

// Bundled model files (committed under the desktop crate's resources).
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
            let base = match EdgeModel::load(CSI_EMBED_V2, PRESENCE_HEAD) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("edge-infer: failed to load base model: {e}");
                    return None;
                }
            };
            let mut per_node = HashMap::new();
            for (id, json) in [(1u8, NODE1_LORA), (2u8, NODE2_LORA)] {
                if let Ok(lora) = LoraAdapter::from_json(json) {
                    per_node.insert(id, base.clone().with_lora(Some(lora)));
                }
            }
            tracing::info!(
                "edge-infer: loaded CSI-embedding model (base + {} per-room adapters)",
                per_node.len()
            );
            Some(Models { base, per_node })
        })
        .as_ref()
}

/// One node's neural inference plus the exact input it ran on.
pub struct NodeInference {
    pub input: [f32; N_FEATURES],
    pub presence: f32,
    pub embedding: Vec<f32>,
    /// Whether a per-room LoRA adapter was applied for this node.
    pub adapted: bool,
}

/// Run inference for `node_id` on an already-assembled 8-feature vector (in
/// [`FEATURE_NAMES`] order). Returns `None` if the model could not be loaded.
pub fn infer(node_id: u8, features: [f32; N_FEATURES]) -> Option<NodeInference> {
    let m = models()?;
    let (model, adapted) = match m.per_node.get(&node_id) {
        Some(model) => (model, true),
        None => (&m.base, false),
    };
    let Inference { embedding, presence } = model.infer(&features);
    Some(NodeInference { input: features, presence, embedding, adapted })
}

/// Label the 8 features for JSON output (name → value).
pub fn labelled_input(features: &[f32; N_FEATURES]) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for (name, v) in FEATURE_NAMES.iter().zip(features.iter()) {
        obj.insert((*name).to_string(), serde_json::json!(v));
    }
    serde_json::Value::Object(obj)
}
