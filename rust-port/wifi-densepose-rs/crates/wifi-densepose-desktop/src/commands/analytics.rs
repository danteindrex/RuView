use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueryRequest {
    pub session_id: String,
    pub vital_summary: serde_json::Value,
    pub pose_anomalies: Vec<String>,
    pub duration_seconds: i64,
    pub csi_snr_db: f64,
    pub csi_vision_image_b64: Option<String>,
}

/// Run full LangGraph multi-agent insight pipeline
#[tauri::command]
pub async fn run_insight_pipeline(request: QueryRequest) -> Result<serde_json::Value, String> {
    let endpoint = std::env::var("RUVIEW_CLOUD_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8001".to_string());
    reqwest::Client::new()
        .post(format!("{}/query", endpoint))
        .json(&request)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())
}

/// Get session insight from cache
#[tauri::command]
pub async fn get_session_insight(session_id: String) -> Result<serde_json::Value, String> {
    let endpoint = std::env::var("RUVIEW_CLOUD_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8001".to_string());
    reqwest::get(format!("{}/insights/{}", endpoint, session_id))
        .await
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())
}

/// Get cross-session trend analytics
#[tauri::command]
pub async fn get_analytics_trends() -> Result<serde_json::Value, String> {
    let endpoint = std::env::var("RUVIEW_CLOUD_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8001".to_string());
    reqwest::get(format!("{}/analytics/trends", endpoint))
        .await
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())
}

/// Get risk distribution across sessions
#[tauri::command]
pub async fn get_risk_distribution() -> Result<serde_json::Value, String> {
    let endpoint = std::env::var("RUVIEW_CLOUD_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8001".to_string());
    reqwest::get(format!("{}/analytics/risk-distribution", endpoint))
        .await
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())
}
