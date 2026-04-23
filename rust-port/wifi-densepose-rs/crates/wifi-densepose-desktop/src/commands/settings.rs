use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Application settings that persist across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub server_http_port: u16,
    pub server_ws_port: u16,
    pub server_udp_port: u16,
    pub server_nexmon_port: u16,
    pub bind_address: String,
    pub server_source: String,
    pub server_tick_ms: u64,
    pub ui_path: String,
    pub server_pi_diag: bool,
    pub server_model_path: String,
    pub server_load_rvf_path: String,
    pub server_save_rvf_path: String,
    pub server_progressive: bool,
    pub server_node_positions: String,
    pub server_calibrate_on_boot: bool,
    pub server_dataset_path: String,
    pub server_dataset_type: String,
    pub server_epochs: usize,
    pub server_pretrain_epochs: usize,
    pub server_checkpoint_dir: String,
    pub server_export_rvf_path: String,
    pub server_build_index: String,
    pub server_enable_benchmark: bool,
    pub server_enable_train: bool,
    pub server_enable_pretrain: bool,
    pub server_enable_embed: bool,
    pub ota_psk: String,
    pub auto_discover: bool,
    pub discover_interval_ms: u32,
    pub theme: String,
    // Raspberry Pi node agent profile fields.
    pub pi_agent_enabled: bool,
    pub pi_agent_listen: String,
    pub pi_agent_aggregator: String,
    pub pi_agent_node_base: u8,
    pub pi_agent_tier: u8,
    pub pi_agent_default_rssi: i8,
    pub pi_agent_noise_floor: i8,
    pub pi_agent_mmwave_mock: bool,
    pub pi_agent_enable_wasm: bool,
    pub pi_agent_wasm_path: String,
    pub pi_agent_wasm_module_id: u8,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            server_http_port: 8080,
            server_ws_port: 8765,
            server_udp_port: 5005,
            server_nexmon_port: 5500,
            bind_address: "127.0.0.1".into(),
            server_source: "auto".into(),
            server_tick_ms: 100,
            ui_path: String::new(),
            server_pi_diag: false,
            server_model_path: String::new(),
            server_load_rvf_path: String::new(),
            server_save_rvf_path: String::new(),
            server_progressive: false,
            server_node_positions: String::new(),
            server_calibrate_on_boot: false,
            server_dataset_path: String::new(),
            server_dataset_type: "mmfi".into(),
            server_epochs: 100,
            server_pretrain_epochs: 50,
            server_checkpoint_dir: String::new(),
            server_export_rvf_path: String::new(),
            server_build_index: String::new(),
            server_enable_benchmark: false,
            server_enable_train: false,
            server_enable_pretrain: false,
            server_enable_embed: false,
            ota_psk: String::new(),
            auto_discover: true,
            discover_interval_ms: 10_000,
            theme: "dark".into(),
            pi_agent_enabled: false,
            pi_agent_listen: "0.0.0.0:5500".into(),
            pi_agent_aggregator: "127.0.0.1:5005".into(),
            pi_agent_node_base: 10,
            pi_agent_tier: 2,
            pi_agent_default_rssi: -55,
            pi_agent_noise_floor: -92,
            pi_agent_mmwave_mock: false,
            pi_agent_enable_wasm: false,
            pi_agent_wasm_path: String::new(),
            pi_agent_wasm_module_id: 1,
        }
    }
}

/// Get the settings file path in the app data directory.
fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    // Ensure directory exists
    fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create app data dir: {}", e))?;

    Ok(app_dir.join("settings.json"))
}

/// Load settings from disk.
#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Option<AppSettings>, String> {
    let path = settings_path(&app)?;

    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read settings: {}", e))?;

    let settings: AppSettings = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse settings: {}", e))?;

    Ok(Some(settings))
}

/// Save settings to disk.
#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = settings_path(&app)?;

    let contents = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    fs::write(&path, contents)
        .map_err(|e| format!("Failed to write settings: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = AppSettings::default();
        assert_eq!(settings.server_http_port, 8080);
        assert_eq!(settings.bind_address, "127.0.0.1");
        assert!(settings.auto_discover);
    }

    #[test]
    fn test_settings_serialization() {
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.server_http_port, settings.server_http_port);
    }
}
