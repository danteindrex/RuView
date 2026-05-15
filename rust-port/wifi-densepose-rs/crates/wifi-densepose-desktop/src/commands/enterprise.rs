use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use tauri::State;
use crate::commands::auth::AuthManager;
use crate::commands::guard;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EnterpriseSettings {
    pub tenant_id: String,
    pub whapi_token: Option<String>,
    pub whapi_number: Option<String>,
    pub alert_target_number: Option<String>,
    pub security_armed: bool,
    pub crowd_threshold: i32,
    pub fall_alert_enabled: bool,
    pub vitals_alert_enabled: bool,
    pub hr_min: f64,
    pub hr_max: f64,
    pub br_min: f64,
    pub br_max: f64,
}

#[tauri::command]
pub async fn get_enterprise_settings(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<EnterpriseSettings, String> {
    let user = guard::require_auth(&access_token, &auth)?;
    let pool = &auth.pool;

    let settings = sqlx::query_as::<_, EnterpriseSettings>(
        r#"
        SELECT 
            tenant_id, whapi_token, whapi_number, alert_target_number,
            security_armed != 0 as security_armed, 
            crowd_threshold,
            fall_alert_enabled != 0 as fall_alert_enabled,
            vitals_alert_enabled != 0 as vitals_alert_enabled,
            hr_min, hr_max, br_min, br_max
        FROM tenant_settings WHERE tenant_id = ?
        "#,
    )
    .bind(&user.tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    match settings {
        Some(s) => Ok(s),
        None => {
            // Return default settings for the tenant
            Ok(EnterpriseSettings {
                tenant_id: user.tenant_id,
                whapi_token: None,
                whapi_number: None,
                alert_target_number: None,
                security_armed: false,
                crowd_threshold: 5,
                fall_alert_enabled: true,
                vitals_alert_enabled: true,
                hr_min: 40.0,
                hr_max: 140.0,
                br_min: 8.0,
                br_max: 30.0,
            })
        }
    }
}

#[tauri::command]
pub async fn save_enterprise_settings(
    access_token: String,
    settings: EnterpriseSettings,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    let user = guard::require_auth(&access_token, &auth)?;
    let pool = &auth.pool;

    // Force the tenant_id to match the user's tenant for security
    let mut s = settings;
    s.tenant_id = user.tenant_id.clone();

    sqlx::query(
        r#"
        INSERT INTO tenant_settings (
            tenant_id, whapi_token, whapi_number, alert_target_number,
            security_armed, crowd_threshold, fall_alert_enabled,
            vitals_alert_enabled, hr_min, hr_max, br_min, br_max
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(tenant_id) DO UPDATE SET
            whapi_token = excluded.whapi_token,
            whapi_number = excluded.whapi_number,
            alert_target_number = excluded.alert_target_number,
            security_armed = excluded.security_armed,
            crowd_threshold = excluded.crowd_threshold,
            fall_alert_enabled = excluded.fall_alert_enabled,
            vitals_alert_enabled = excluded.vitals_alert_enabled,
            hr_min = excluded.hr_min,
            hr_max = excluded.hr_max,
            br_min = excluded.br_min,
            br_max = excluded.br_max,
            updated_at = datetime('now')
        "#,
    )
    .bind(&s.tenant_id)
    .bind(&s.whapi_token)
    .bind(&s.whapi_number)
    .bind(&s.alert_target_number)
    .bind(s.security_armed as i32)
    .bind(s.crowd_threshold)
    .bind(s.fall_alert_enabled as i32)
    .bind(s.vitals_alert_enabled as i32)
    .bind(s.hr_min)
    .bind(s.hr_max)
    .bind(s.br_min)
    .bind(s.br_max)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn whapi_get_qr(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<String, String> {
    let user = guard::require_auth(&access_token, &auth)?;
    let pool = &auth.pool;

    #[derive(sqlx::FromRow)]
    struct WhapiTokenRow { whapi_token: Option<String> }

    let settings = sqlx::query_as::<_, WhapiTokenRow>(
        "SELECT whapi_token FROM tenant_settings WHERE tenant_id = ?",
    )
    .bind(&user.tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|e: sqlx::Error| e.to_string())?
    .ok_or("WhatsApp not configured for this tenant")?;

    let token = settings.whapi_token.ok_or("Whapi token missing")?;

    let client = reqwest::Client::new();
    let res = client
        .get("https://gate.whapi.cloud/users/login")
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Whapi request failed: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_body = res.text().await.unwrap_or_default();
        return Err(format!("Whapi login error ({}): {}", status, err_body));
    }

    let json: serde_json::Value = res.json().await.map_err(|e| format!("Whapi JSON parse error: {}", e))?;
    
    // Whapi returns a base64 QR or a URL. 
    // Usually it's in data.qr_code or similar.
    let qr = json["data"]["qr_code"]
        .as_str()
        .or_else(|| json["qr_code"].as_str())
        .or_else(|| json["data"]["qr"].as_str())
        .or_else(|| json["qr"].as_str())
        .ok_or_else(|| format!("QR code not found in response: {}", json))?;
    
    // Ensure data URI prefix if missing and not a URL
    let final_qr = if qr.starts_with("data:image") || qr.starts_with("http") {
        qr.to_string()
    } else {
        format!("data:image/png;base64,{}", qr)
    };

    Ok(final_qr)
}

#[tauri::command]
pub async fn whapi_send_test(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    whapi_send_alert(access_token, "🚨 RuView Enterprise: Test notification successful.".into(), auth).await
}

#[tauri::command]
pub async fn whapi_send_alert(
    access_token: String,
    message: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    let user = guard::require_auth(&access_token, &auth)?;
    let pool = &auth.pool;

    #[derive(sqlx::FromRow)]
    struct AlertRow { whapi_token: Option<String>, alert_target_number: Option<String> }

    let settings = sqlx::query_as::<_, AlertRow>(
        "SELECT whapi_token, alert_target_number FROM tenant_settings WHERE tenant_id = ?",
    )
    .bind(&user.tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|e: sqlx::Error| e.to_string())?
    .ok_or("Tenant settings not found")?;

    let token = settings.whapi_token.ok_or("Whapi token missing")?;
    let target = settings.alert_target_number.ok_or("Target number missing")?;

    let client = reqwest::Client::new();
    let res = client
        .post("https://gate.whapi.cloud/messages/text")
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "typing_time": 0,
            "to": target,
            "body": message
        }))
        .send()
        .await
        .map_err(|e| format!("Whapi request failed: {}", e))?;

    let status = res.status();
    if !status.is_success() {
        let err_body = res.text().await.unwrap_or_default();
        return Err(format!("Whapi error ({}): {}", status, err_body));
    }

    Ok(())
}
