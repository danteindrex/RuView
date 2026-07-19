use serde::{Deserialize, Serialize};
use tauri::Manager;
use crate::deployment::DeploymentInfo;
use super::frappe_client as fc;

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

/// Frappe resource response shape for RuView Deployment
#[derive(Debug, Deserialize)]
struct FrappeDeployment {
    deployment_id: Option<String>,
    deployment_name: Option<String>,
    location_name: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    tenant_id: Option<String>,
    last_seen: Option<String>,
    node_count: Option<u32>,
    active_risk_level: Option<String>,
    status: Option<String>,
}

impl From<FrappeDeployment> for DeploymentStatus {
    fn from(d: FrappeDeployment) -> Self {
        let online = d.status.as_deref() == Some("Online");
        DeploymentStatus {
            deployment_id: d.deployment_id.unwrap_or_default(),
            deployment_name: d.deployment_name.unwrap_or_default(),
            location_name: d.location_name.unwrap_or_default(),
            latitude: d.latitude,
            longitude: d.longitude,
            tenant_id: d.tenant_id,
            last_seen: d.last_seen,
            node_count: d.node_count,
            active_risk_level: d.active_risk_level,
            online: Some(online),
        }
    }
}

/// Get this deployment's identity info (local only)
#[tauri::command]
pub async fn get_deployment_info(app: tauri::AppHandle) -> Result<DeploymentInfo, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(crate::deployment::load_or_create(&dir))
}

/// Update this deployment's name and location (persisted locally)
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

/// Register or heartbeat this deployment with Frappe
#[tauri::command]
pub async fn register_deployment(app: tauri::AppHandle) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let info = crate::deployment::load_or_create(&dir);
    let body = serde_json::json!({
        "deployment_id": info.deployment_id,
        "deployment_name": info.deployment_name,
        "location_name": info.location_name,
        "latitude": info.latitude,
        "longitude": info.longitude,
        "tenant_id": info.tenant_id,
    });
    let resp = fc::post_method("ruview_care.ruview_care.api.register_deployment")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let msg = fc::unwrap_message(resp).await?;
    Ok(msg.get("name").and_then(|v| v.as_str()).unwrap_or("registered").to_string())
}

/// List all deployments for this tenant (Enterprise plan)
#[tauri::command]
pub async fn list_deployments(tenant_id: String) -> Result<Vec<DeploymentStatus>, String> {
    use serde_json::json;
    let filters = json!([["tenant_id", "=", tenant_id]]).to_string();
    let fields = json!(["deployment_id","deployment_name","location_name","latitude","longitude",
                         "tenant_id","last_seen","node_count","active_risk_level","status"])
        .to_string();
    let resp = fc::get_resource("RuView Deployment")
        .query(&[("filters", &filters), ("fields", &fields), ("limit", &"500".to_string())])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let data = fc::unwrap_data(resp).await?;
    let docs: Vec<FrappeDeployment> = serde_json::from_value(data).map_err(|e| e.to_string())?;
    Ok(docs.into_iter().map(DeploymentStatus::from).collect())
}

/// Get aggregate status across all deployments for a tenant
#[tauri::command]
pub async fn get_deployments_aggregate(tenant_id: String) -> Result<serde_json::Value, String> {
    let resp = fc::get_method("ruview_care.ruview_care.api.get_deployments_summary")
        .query(&[("tenant_id", &tenant_id)])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}
