//! Enterprise alerting (backend "D" + whapi.cloud) — the desktop's WhatsApp
//! alert settings. Per-tenant config lives in `wave.db` (`tenant_settings`);
//! QR-login and message sends hit gate.whapi.cloud with the tenant's token.
//!
//! Reads/writes the same `tenant_settings` rows the desktop's enterprise page
//! uses. The two message senders (`test`, `send`) are real outward-facing side
//! effects — the caller confirms before invoking them, and the token is never
//! printed.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::Row;

use crate::admin;

/// Read a tenant's alert settings (defaults when the row is absent).
pub async fn settings_get(tenant_id: &str) -> Result<Value> {
    let pool = admin::open().await?;
    let row = sqlx::query(
        "SELECT whapi_number, alert_target_number, security_armed, crowd_threshold, \
         fall_alert_enabled, vitals_alert_enabled, hr_min, hr_max, br_min, br_max, \
         CASE WHEN whapi_token IS NULL OR whapi_token = '' THEN 0 ELSE 1 END AS has_token \
         FROM tenant_settings WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await?;
    Ok(match row {
        Some(r) => json!({
            "tenant_id": tenant_id,
            "whapi_configured": r.get::<i64, _>("has_token") == 1,
            "whapi_number": r.get::<Option<String>, _>("whapi_number"),
            "alert_target_number": r.get::<Option<String>, _>("alert_target_number"),
            "security_armed": r.get::<i64, _>("security_armed") != 0,
            "crowd_threshold": r.get::<i64, _>("crowd_threshold"),
            "fall_alert_enabled": r.get::<i64, _>("fall_alert_enabled") != 0,
            "vitals_alert_enabled": r.get::<i64, _>("vitals_alert_enabled") != 0,
            "hr_min": r.get::<f64, _>("hr_min"), "hr_max": r.get::<f64, _>("hr_max"),
            "br_min": r.get::<f64, _>("br_min"), "br_max": r.get::<f64, _>("br_max"),
        }),
        None => json!({
            "tenant_id": tenant_id, "whapi_configured": false,
            "security_armed": false, "crowd_threshold": 5,
            "fall_alert_enabled": true, "vitals_alert_enabled": true,
            "hr_min": 40.0, "hr_max": 140.0, "br_min": 8.0, "br_max": 30.0,
            "note": "no tenant_settings row yet — showing defaults",
        }),
    })
}

/// Upsert the WhatsApp credentials + target number for a tenant. Other alert
/// fields keep their existing values (or the desktop defaults on first insert).
pub async fn settings_set(
    tenant_id: &str,
    whapi_token: Option<&str>,
    whapi_number: Option<&str>,
    target_number: Option<&str>,
) -> Result<()> {
    let pool = admin::open().await?;
    // Insert defaults if absent, then patch only the provided fields via COALESCE.
    sqlx::query(
        "INSERT INTO tenant_settings (tenant_id, whapi_token, whapi_number, alert_target_number) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(tenant_id) DO UPDATE SET \
           whapi_token = COALESCE(?2, whapi_token), \
           whapi_number = COALESCE(?3, whapi_number), \
           alert_target_number = COALESCE(?4, alert_target_number), \
           updated_at = datetime('now')",
    )
    .bind(tenant_id)
    .bind(whapi_token)
    .bind(whapi_number)
    .bind(target_number)
    .execute(&pool)
    .await
    .context("writing tenant_settings")?;
    Ok(())
}

async fn tenant_token(tenant_id: &str) -> Result<String> {
    let pool = admin::open().await?;
    let token: Option<String> = sqlx::query_scalar(
        "SELECT whapi_token FROM tenant_settings WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await?
    .flatten();
    token
        .filter(|t| !t.is_empty())
        .context("no whapi_token for this tenant — set one with `wave-cli alerts set`")
}

/// Fetch a WhatsApp login QR (base64/data-URI) for the tenant's whapi account.
pub async fn qr(tenant_id: &str) -> Result<Value> {
    let token = tenant_token(tenant_id).await?;
    let res = reqwest::Client::new()
        .get("https://gate.whapi.cloud/users/login")
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .context("whapi login request")?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("whapi login {status}: {}", body.trim());
    }
    let j: Value = res.json().await.context("whapi JSON")?;
    let qr = j["data"]["qr_code"]
        .as_str()
        .or_else(|| j["qr_code"].as_str())
        .or_else(|| j["data"]["qr"].as_str())
        .or_else(|| j["qr"].as_str())
        .context("QR code not found in whapi response")?;
    let data_uri = if qr.starts_with("data:image") || qr.starts_with("http") {
        qr.to_string()
    } else {
        format!("data:image/png;base64,{qr}")
    };
    Ok(json!({ "qr": data_uri }))
}

/// Send a WhatsApp message to the tenant's configured alert target. Outward-facing
/// side effect — the caller must confirm first.
pub async fn send(tenant_id: &str, message: &str) -> Result<()> {
    let token = tenant_token(tenant_id).await?;
    let pool = admin::open().await?;
    let target: Option<String> = sqlx::query_scalar(
        "SELECT alert_target_number FROM tenant_settings WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await?
    .flatten();
    let target = target
        .filter(|t| !t.is_empty())
        .context("no alert_target_number for this tenant")?;

    let res = reqwest::Client::new()
        .post("https://gate.whapi.cloud/messages/text")
        .header("Authorization", format!("Bearer {token}"))
        .json(&json!({ "typing_time": 0, "to": target, "body": message }))
        .send()
        .await
        .context("whapi send request")?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("whapi send {status}: {}", body.trim());
    }
    Ok(())
}
