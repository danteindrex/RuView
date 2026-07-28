use serde::{Deserialize, Serialize};
use super::frappe_client as fc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QueryRequest {
    pub session_id: String,
    pub deployment_id: String,
    pub vital_summary: serde_json::Value,
    pub pose_anomalies: Vec<String>,
    pub duration_seconds: i64,
    pub csi_snr_db: f64,
}

/// Ingest a CSI session and enqueue the LangGraph pipeline in Frappe.
/// Returns immediately; poll get_session_insight for the result.
#[tauri::command]
pub async fn run_insight_pipeline(request: QueryRequest) -> Result<serde_json::Value, String> {
    use serde_json::json;
    let body = json!({
        "session_id": request.session_id,
        "deployment_id": request.deployment_id,
        "vital_summary": request.vital_summary.to_string(),
        "pose_anomalies": serde_json::to_string(&request.pose_anomalies).unwrap_or_default(),
        "duration_seconds": request.duration_seconds,
        "csi_snr_db": request.csi_snr_db,
    });
    let resp = fc::post_method("wave_care.wave_care.api.ingest_csi_session")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}

/// Poll for the Insight Report for a given session_id.
/// Returns null (JSON) until the pipeline completes.
#[tauri::command]
pub async fn get_session_insight(session_id: String) -> Result<serde_json::Value, String> {
    let resp = fc::get_method("wave_care.wave_care.api.get_insight_by_session_id")
        .query(&[("session_id", &session_id)])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}

/// HR/BR trend data across recent sessions (sparkline data)
#[tauri::command]
pub async fn get_analytics_trends() -> Result<serde_json::Value, String> {
    let resp = fc::get_method("wave_care.wave_care.api.get_analytics_trends")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}

/// Risk level distribution across all sessions (bar chart data)
#[tauri::command]
pub async fn get_risk_distribution() -> Result<serde_json::Value, String> {
    let resp = fc::get_method("wave_care.wave_care.api.get_risk_distribution")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}
