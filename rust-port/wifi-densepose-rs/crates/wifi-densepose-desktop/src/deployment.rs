use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentInfo {
    pub deployment_id: String,
    pub deployment_name: String,
    pub location_name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub tenant_id: Option<String>,
}

impl DeploymentInfo {
    pub fn new_local() -> Self {
        Self {
            deployment_id: Uuid::new_v4().to_string(),
            deployment_name: "Primary Deployment".to_string(),
            location_name: "Unknown Location".to_string(),
            latitude: None,
            longitude: None,
            tenant_id: None,
        }
    }
}

const DEPLOYMENT_FILE: &str = "deployment.json";

pub fn load_or_create(app_data_dir: &Path) -> DeploymentInfo {
    let path = app_data_dir.join(DEPLOYMENT_FILE);
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(info) = serde_json::from_slice::<DeploymentInfo>(&bytes) {
            return info;
        }
    }
    let info = DeploymentInfo::new_local();
    let _ = std::fs::write(&path, serde_json::to_vec_pretty(&info).unwrap_or_default());
    info
}

pub fn save(app_data_dir: &Path, info: &DeploymentInfo) -> Result<(), String> {
    let path = app_data_dir.join(DEPLOYMENT_FILE);
    std::fs::write(&path, serde_json::to_vec_pretty(info).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
