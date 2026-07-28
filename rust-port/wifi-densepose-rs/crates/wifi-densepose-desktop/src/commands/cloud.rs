use tauri::State;
use crate::state::AppState;

#[tauri::command]
pub async fn set_consent(granted: bool) -> Result<(), String> {
    // TODO: write to settings table via AppState DB pool
    tracing::info!("cloud consent: granted={}", granted);
    Ok(())
}

#[tauri::command]
pub async fn get_cloud_config() -> Result<serde_json::Value, String> {
    // Report the real configured state, not hardcoded flags: cloud is "enabled"
    // only when an endpoint is actually configured. Consent is not persisted
    // yet, so it is reported as null (unknown) rather than a fabricated false.
    let endpoint = std::env::var("WAVE_CLOUD_ENDPOINT").unwrap_or_default();
    Ok(serde_json::json!({
        "endpoint": endpoint,
        "consent_granted": serde_json::Value::Null,
        "enabled": !endpoint.is_empty(),
    }))
}

#[tauri::command]
pub async fn upload_sensing_session(
    app: tauri::AppHandle,
    _state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    // Do NOT upload a fabricated stub and report success. Real session
    // serialization (pose/vitals/CSI capture → payload) is not wired yet, so
    // this fails honestly instead of silently "uploading" empty data.
    let _ = (app, session_id);
    Err("Cloud session upload is not available yet: real captured-session \
         serialization is not implemented. No data was uploaded.".to_string())
}
