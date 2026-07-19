use tauri::State;
use crate::state::AppState;
use crate::cloud::InsightUploader;

#[tauri::command]
pub async fn set_consent(granted: bool) -> Result<(), String> {
    // TODO: write to settings table via AppState DB pool
    tracing::info!("cloud consent: granted={}", granted);
    Ok(())
}

#[tauri::command]
pub async fn get_cloud_config() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "endpoint": std::env::var("RUVIEW_CLOUD_ENDPOINT").unwrap_or_default(),
        "consent_granted": false,
        "enabled": false,
    }))
}

#[tauri::command]
pub async fn upload_sensing_session(
    _state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    // TODO: check consent_granted from settings table
    // TODO: check plan tier (feature/cloud-tier-gating)
    let endpoint = std::env::var("RUVIEW_CLOUD_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8001".to_string());
    let signing_key = std::env::var("RUVIEW_SIGNING_KEY")
        .unwrap_or_default()
        .into_bytes();
    let stub = serde_json::json!({"session_id": &session_id, "stub": true});
    let session_json = serde_json::to_vec(&stub).map_err(|e| e.to_string())?;
    InsightUploader::new(endpoint, signing_key)
        .upload_session(&session_id, &session_json)
        .await
}
