use serde::{Deserialize, Serialize};
use tauri::Manager;
use crate::deployment::DeploymentInfo;

#[derive(Debug, Serialize, Deserialize)]
pub struct DeploymentStatus {
    pub deployment_id: String,
    pub deployment_name: String,
    pub location_name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub tenant_id: Option<String>,
    pub last_seen: Option<String>,
    pub node_count: Option<u32>,
    pub active_risk_level: Option<String>,
    pub online: Option<bool>,
}

/// Get this deployment's identity info
#[tauri::command]
pub async fn get_deployment_info(app: tauri::AppHandle) -> Result<DeploymentInfo, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(crate::deployment::load_or_create(&dir))
}

/// Update this deployment's name and location (admin only — enforcement is server-side)
#[tauri::command]
pub async fn set_deployment_info(
    app: tauri::AppHandle,
    deployment_name: String,
    location_name: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    tenant_id: Option<String>,
) -> Result<DeploymentInfo, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut info = crate::deployment::load_or_create(&dir);
    info.deployment_name = deployment_name;
    info.location_name = location_name;
    info.latitude = latitude;
    info.longitude = longitude;
    info.tenant_id = tenant_id;
    crate::deployment::save(&dir, &info)?;
    Ok(info)
}

/// Register or heartbeat this deployment with the cloud backend
#[tauri::command]
pub async fn register_deployment(app: tauri::AppHandle) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let info = crate::deployment::load_or_create(&dir);
    let endpoint = std::env::var("RUVIEW_CLOUD_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8001".to_string());
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "deployment_id": info.deployment_id,
        "deployment_name": info.deployment_name,
        "location_name": info.location_name,
        "latitude": info.latitude,
        "longitude": info.longitude,
        "tenant_id": info.tenant_id,
    });
    let resp = client
        .post(format!("{}/deployments/register", endpoint))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    if status.is_success() {
        Ok("registered".to_string())
    } else {
        Err(format!("Registration failed: {}", status))
    }
}

/// List all deployments for this tenant (Enterprise plan only — enforced server-side)
#[tauri::command]
pub async fn list_deployments(tenant_id: String) -> Result<Vec<DeploymentStatus>, String> {
    let endpoint = std::env::var("RUVIEW_CLOUD_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8001".to_string());
    reqwest::Client::new()
        .get(format!("{}/deployments?tenant_id={}", endpoint, tenant_id))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<Vec<DeploymentStatus>>()
        .await
        .map_err(|e| e.to_string())
}

/// Get aggregate status across all deployments for a tenant
#[tauri::command]
pub async fn get_deployments_aggregate(tenant_id: String) -> Result<serde_json::Value, String> {
    let endpoint = std::env::var("RUVIEW_CLOUD_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8001".to_string());
    reqwest::Client::new()
        .get(format!("{}/deployments/aggregate?tenant_id={}", endpoint, tenant_id))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())
}
