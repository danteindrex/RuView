//! Frappe / cloud-backend commands (backend "E") — the enterprise tier the
//! desktop's medical / multi-location pages talk to. All additive: credentials
//! live in the same OS keychain service the desktop uses (`wave-frappe`), and
//! the deployment identity is the same `deployment.json` in the app data dir, so
//! anything configured here is picked up by the app and vice-versa.
//!
//! The insight / deployment / register verbs need a reachable Frappe backend;
//! they compile and error cleanly when it is absent (marked `coupled*` in
//! `coverage`). Config get/set and the local deployment identity work offline.

use anyhow::{Context, Result};
use keyring::Entry;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const SERVICE: &str = "wave-frappe";
const KEY_URL: &str = "frappe-url";
const KEY_API_KEY: &str = "frappe-api-key";
const KEY_API_SECRET: &str = "frappe-api-secret";

// ── config (keyring) ─────────────────────────────────────────────────────────

/// The app data dir shared with the desktop (`%APPDATA%\net.ruv.wave` / XDG).
fn app_data_dir() -> Result<std::path::PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(std::path::PathBuf::from)
    } else {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
    }
    .context("cannot resolve the app data directory")?;
    Ok(base.join("net.ruv.wave"))
}

/// Load Frappe creds from the keychain into env (mirrors the desktop's
/// `load_frappe_config_into_env`) so the REST client below can auth.
fn load_env() {
    for (k, var) in [
        (KEY_URL, "WAVE_FRAPPE_URL"),
        (KEY_API_KEY, "WAVE_FRAPPE_API_KEY"),
        (KEY_API_SECRET, "WAVE_FRAPPE_API_SECRET"),
    ] {
        if std::env::var(var).is_err() {
            if let Ok(val) = Entry::new(SERVICE, k).and_then(|e| e.get_password()) {
                if !val.is_empty() {
                    std::env::set_var(var, val);
                }
            }
        }
    }
}

/// Show the configured Frappe URL + a masked key hint (never the full secret).
pub fn config_get() -> Result<Value> {
    let url = Entry::new(SERVICE, KEY_URL)
        .and_then(|e| e.get_password())
        .unwrap_or_else(|_| {
            std::env::var("WAVE_FRAPPE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
        });
    let api_key = Entry::new(SERVICE, KEY_API_KEY)
        .and_then(|e| e.get_password())
        .unwrap_or_else(|_| std::env::var("WAVE_FRAPPE_API_KEY").unwrap_or_default());
    let configured = !api_key.is_empty();
    let hint = if api_key.len() > 6 {
        format!("{}…", &api_key[..6])
    } else if !api_key.is_empty() {
        "set".to_string()
    } else {
        String::new()
    };
    Ok(json!({ "url": url, "api_key_hint": hint, "configured": configured }))
}

/// Persist Frappe URL + API key/secret to the keychain (same store the app reads).
pub fn config_set(url: &str, api_key: Option<&str>, api_secret: Option<&str>) -> Result<()> {
    Entry::new(SERVICE, KEY_URL)
        .and_then(|e| e.set_password(url))
        .map_err(|e| anyhow::anyhow!("keychain write (url): {e}"))?;
    if let Some(k) = api_key.filter(|s| !s.is_empty()) {
        Entry::new(SERVICE, KEY_API_KEY)
            .and_then(|e| e.set_password(k))
            .map_err(|e| anyhow::anyhow!("keychain write (api key): {e}"))?;
    }
    if let Some(s) = api_secret.filter(|s| !s.is_empty()) {
        Entry::new(SERVICE, KEY_API_SECRET)
            .and_then(|e| e.set_password(s))
            .map_err(|e| anyhow::anyhow!("keychain write (api secret): {e}"))?;
    }
    Ok(())
}

// ── Frappe REST client (matches desktop `frappe_client.rs`) ───────────────────

fn frappe_url() -> String {
    std::env::var("WAVE_FRAPPE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

fn auth_header() -> Option<String> {
    let key = std::env::var("WAVE_FRAPPE_API_KEY").ok()?;
    let secret = std::env::var("WAVE_FRAPPE_API_SECRET").ok()?;
    (!key.is_empty() && !secret.is_empty()).then(|| format!("token {key}:{secret}"))
}

fn with_auth(req: RequestBuilder) -> RequestBuilder {
    match auth_header() {
        Some(a) => req.header("Authorization", a),
        None => req,
    }
}

fn post_method(method: &str) -> RequestBuilder {
    with_auth(Client::new().post(format!("{}/api/method/{method}", frappe_url())))
}

fn get_method(method: &str) -> RequestBuilder {
    with_auth(Client::new().get(format!("{}/api/method/{method}", frappe_url())))
}

fn get_resource(doctype: &str) -> RequestBuilder {
    let encoded = doctype.replace(' ', "%20");
    with_auth(Client::new().get(format!("{}/api/resource/{encoded}", frappe_url())))
}

async fn unwrap_message(resp: reqwest::Response) -> Result<Value> {
    let status = resp.status();
    let body: Value = resp.json().await.context("parsing Frappe response")?;
    if status.is_success() {
        Ok(body.get("message").cloned().unwrap_or(body))
    } else {
        let msg = body
            .get("exception")
            .or_else(|| body.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Frappe error")
            .to_string();
        anyhow::bail!("Frappe {status}: {msg}")
    }
}

async fn unwrap_data(resp: reqwest::Response) -> Result<Value> {
    let status = resp.status();
    let body: Value = resp.json().await.context("parsing Frappe response")?;
    if status.is_success() {
        Ok(body.get("data").cloned().unwrap_or(body))
    } else {
        anyhow::bail!("Frappe error: {status}")
    }
}

// ── insight / analytics ───────────────────────────────────────────────────────

pub async fn insight_run(
    session_id: &str,
    deployment_id: &str,
    duration_seconds: i64,
    csi_snr_db: f64,
    vital_summary: Value,
    pose_anomalies: Vec<String>,
) -> Result<Value> {
    load_env();
    let body = json!({
        "session_id": session_id,
        "deployment_id": deployment_id,
        "vital_summary": vital_summary.to_string(),
        "pose_anomalies": serde_json::to_string(&pose_anomalies).unwrap_or_default(),
        "duration_seconds": duration_seconds,
        "csi_snr_db": csi_snr_db,
    });
    let resp = post_method("wave_care.wave_care.api.ingest_csi_session")
        .json(&body)
        .send()
        .await
        .context("POST ingest_csi_session")?;
    unwrap_message(resp).await
}

pub async fn insight_get(session_id: &str) -> Result<Value> {
    load_env();
    let resp = get_method("wave_care.wave_care.api.get_insight_by_session_id")
        .query(&[("session_id", session_id)])
        .send()
        .await
        .context("GET get_insight_by_session_id")?;
    unwrap_message(resp).await
}

pub async fn insight_trends() -> Result<Value> {
    load_env();
    let resp = get_method("wave_care.wave_care.api.get_analytics_trends")
        .send()
        .await
        .context("GET get_analytics_trends")?;
    unwrap_message(resp).await
}

pub async fn insight_risk() -> Result<Value> {
    load_env();
    let resp = get_method("wave_care.wave_care.api.get_risk_distribution")
        .send()
        .await
        .context("GET get_risk_distribution")?;
    unwrap_message(resp).await
}

// ── deployment identity (local deployment.json + Frappe registry) ─────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentInfo {
    pub deployment_id: String,
    pub deployment_name: String,
    pub location_name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub tenant_id: Option<String>,
}

fn deployment_path() -> Result<std::path::PathBuf> {
    Ok(app_data_dir()?.join("deployment.json"))
}

/// Read the local deployment identity, creating a default one if absent — the
/// same file (`deployment.json`) and schema the desktop uses.
pub fn deployment_load_or_create() -> Result<DeploymentInfo> {
    let path = deployment_path()?;
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(info) = serde_json::from_slice::<DeploymentInfo>(&bytes) {
            return Ok(info);
        }
    }
    let info = DeploymentInfo {
        deployment_id: uuid::Uuid::new_v4().to_string(),
        deployment_name: "Primary Deployment".to_string(),
        location_name: "Unknown Location".to_string(),
        latitude: None,
        longitude: None,
        tenant_id: None,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&info)?)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(info)
}

pub fn deployment_set(
    name: Option<&str>,
    location: Option<&str>,
    lat: Option<f64>,
    lon: Option<f64>,
    tenant: Option<&str>,
) -> Result<DeploymentInfo> {
    let mut info = deployment_load_or_create()?;
    if let Some(n) = name {
        info.deployment_name = n.to_string();
    }
    if let Some(l) = location {
        info.location_name = l.to_string();
    }
    if lat.is_some() {
        info.latitude = lat;
    }
    if lon.is_some() {
        info.longitude = lon;
    }
    if let Some(t) = tenant {
        info.tenant_id = Some(t.to_string());
    }
    std::fs::write(deployment_path()?, serde_json::to_vec_pretty(&info)?)?;
    Ok(info)
}

/// Register / heartbeat this deployment with Frappe.
pub async fn deployment_register() -> Result<Value> {
    load_env();
    let info = deployment_load_or_create()?;
    let body = json!({
        "deployment_id": info.deployment_id,
        "deployment_name": info.deployment_name,
        "location_name": info.location_name,
        "latitude": info.latitude,
        "longitude": info.longitude,
        "tenant_id": info.tenant_id,
    });
    let resp = post_method("wave_care.wave_care.api.register_deployment")
        .json(&body)
        .send()
        .await
        .context("POST register_deployment")?;
    unwrap_message(resp).await
}

pub async fn deployment_list(tenant_id: &str) -> Result<Value> {
    load_env();
    let filters = json!([["tenant_id", "=", tenant_id]]).to_string();
    let fields = json!([
        "deployment_id", "deployment_name", "location_name", "latitude", "longitude",
        "tenant_id", "last_seen", "node_count", "active_risk_level", "status"
    ])
    .to_string();
    let resp = get_resource("Wave Deployment")
        .query(&[("filters", filters.as_str()), ("fields", fields.as_str()), ("limit", "500")])
        .send()
        .await
        .context("GET Wave Deployment")?;
    let data = unwrap_data(resp).await?;
    Ok(json!({ "deployments": data }))
}

pub async fn deployment_aggregate(tenant_id: &str) -> Result<Value> {
    load_env();
    let resp = get_method("wave_care.wave_care.api.get_deployments_summary")
        .query(&[("tenant_id", tenant_id)])
        .send()
        .await
        .context("GET get_deployments_summary")?;
    unwrap_message(resp).await
}

// ── cloud config (read-only) ──────────────────────────────────────────────────

/// Cloud upload endpoint + whether a signing key is present. (Consent + upload
/// are desktop-only: the desktop's `set_consent` is a no-op stub and upload is
/// bound to the app's deployment identity + HMAC signing key — see `coverage`.)
pub fn cloud_config() -> Value {
    let endpoint = std::env::var("WAVE_CLOUD_ENDPOINT").unwrap_or_default();
    let has_signing_key = std::env::var("WAVE_SIGNING_KEY")
        .map(|k| !k.is_empty())
        .unwrap_or(false);
    json!({
        "endpoint": endpoint,
        "signing_key_present": has_signing_key,
        "enabled": !endpoint.is_empty(),
    })
}
