//! Bundled pretrained models shipped inside the desktop app.
//!
//! The `ruvnet/wifi-densepose-pretrained` model files (contrastive CSI encoder +
//! quantized edge variants + per-node LoRA adapters, ~165 KB total) are bundled
//! as Tauri resources (`bundle.resources` in `tauri.conf.json`, staged under
//! `resources/models/`). This module exposes them to the frontend so they are
//! discoverable offline without a HuggingFace download.
//!
//! NB: nothing yet *runs* these models — the sensing pipeline is DSP-based and
//! the ESP32 has no native inference. Bundling makes the weights present and
//! addressable; wiring them into hub/Pi inference is separate follow-up work.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// One bundled model file (name, size, absolute on-disk path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledModel {
    pub name: String,
    pub size_bytes: u64,
    pub path: String,
}

/// Resolve the directory holding the bundled model files.
///
/// In a packaged app the resources live under `resource_dir()/resources/models`.
/// In a plain `cargo run`/portable build the resource dir resolves to the crate
/// (or exe) directory, so `resources/models` sits alongside — try both.
fn bundled_models_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    let primary = resource_dir.join("resources").join("models");
    if primary.is_dir() {
        return Some(primary);
    }
    // Fallback: some bundlers flatten resources directly under the resource dir.
    let flat = resource_dir.join("models");
    if flat.is_dir() {
        return Some(flat);
    }
    None
}

/// List the pretrained model files bundled with the app.
///
/// Read-only enumeration of shipped weight/config files; no auth required
/// (they contain no secrets and are baked into the installer).
#[tauri::command]
pub fn list_bundled_models(app: AppHandle) -> Result<Vec<BundledModel>, String> {
    let dir = match bundled_models_dir(&app) {
        Some(d) => d,
        None => return Ok(Vec::new()),
    };
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| format!("Cannot read bundled models dir {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        out.push(BundledModel {
            name,
            size_bytes,
            path: path.to_string_lossy().to_string(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}
