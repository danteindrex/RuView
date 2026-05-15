//! License validation and activation against the cloud License Server.
//!
//! Handles:
//! - First-time license activation (binds hardware fingerprint)
//! - Startup license validation (phone-home)
//! - Offline grace period (7 days)
//! - Caching validated license in local SQLite

use crate::auth::models::{License, LicenseStatus};
use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

/// License server base URL (configurable via env).
const DEFAULT_LICENSE_SERVER_URL: &str = "https://license.wave.io";

/// Offline grace period: 7 days.
const GRACE_PERIOD_DAYS: i64 = 7;

/// Response from the license server's validate/activate endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct LicenseServerResponse {
    pub valid: bool,
    pub tenant: Option<TenantInfo>,
    pub tier: Option<String>,
    pub modules: Option<Vec<String>>,
    pub max_users: Option<i32>,
    pub max_nodes: Option<i32>,
    pub expires_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TenantInfo {
    pub id: String,
    pub name: String,
    pub industry: Option<String>,
}

/// Get the hardware fingerprint for this machine.
pub fn get_hardware_fingerprint() -> String {
    match machine_uid::get() {
        Ok(uid) => {
            // Hash the UID for privacy
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(uid.as_bytes());
            hex::encode(hasher.finalize())
        }
        Err(e) => {
            tracing::warn!("Could not read machine UID: {}", e);
            "unknown".to_string()
        }
    }
}

/// Get the license server URL from env or default.
fn license_server_url() -> String {
    std::env::var("WAVE_LICENSE_SERVER_URL")
        .unwrap_or_else(|_| DEFAULT_LICENSE_SERVER_URL.to_string())
}

/// Activate a license key with the cloud server.
///
/// This is called on first launch. It:
/// 1. Sends the license key + hardware fingerprint to the server
/// 2. Receives tenant info, allowed modules, etc.
/// 3. Caches the response in local SQLite
pub async fn activate_license(
    pool: &SqlitePool,
    license_key: &str,
) -> Result<LicenseServerResponse, String> {
    #[cfg(debug_assertions)]
    {
        tracing::warn!("Bypassing actual license server HTTP check for DEV mode.");
        let body = LicenseServerResponse {
            valid: true,
            tenant: Some(TenantInfo {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Development Tenant".to_string(),
                industry: Some("Software".to_string()),
            }),
            tier: Some("enterprise".to_string()),
            modules: Some(vec!["wifi-sensing".to_string(), "ota".to_string()]),
            max_users: Some(50),
            max_nodes: Some(10),
            expires_at: Some(chrono::Utc::now().checked_add_months(chrono::Months::new(12)).unwrap().format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            error: None,
        };

        let tenant = body.tenant.as_ref().unwrap();
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let id = Uuid::new_v4().to_string();
        let modules_json = serde_json::to_string(&body.modules).unwrap_or_default();
        let cached = serde_json::to_string(&body).unwrap_or_default();

        sqlx::query(
            "INSERT INTO licenses \
             (id, license_key, tenant_id, tenant_name, tier, hardware_fingerprint, \
              allowed_modules, max_users, issued_at, expires_at, last_validated_at, \
              is_active, cached_response) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
        )
        .bind(&id)
        .bind(license_key)
        .bind(&tenant.id)
        .bind(&tenant.name)
        .bind(body.tier.as_deref().unwrap_or("starter"))
        .bind(&get_hardware_fingerprint())
        .bind(&modules_json)
        .bind(body.max_users.unwrap_or(5))
        .bind(&now)
        .bind(body.expires_at.as_deref().unwrap_or(&now))
        .bind(&now)
        .bind(&cached)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to cache license: {}", e))?;

        return Ok(body);
    }

    #[cfg(not(debug_assertions))]
    {
        let fingerprint = get_hardware_fingerprint();
        let url = format!("{}/api/v1/license/activate", license_server_url());

        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .json(&serde_json::json!({
                "license_key": license_key,
                "hardware_fingerprint": fingerprint,
                "app_version": env!("CARGO_PKG_VERSION"),
            }))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("Cannot reach license server: {}", e))?;

        let status = response.status();
        let body: LicenseServerResponse = response
            .json()
            .await
            .map_err(|e| format!("Invalid license server response: {}", e))?;

        if !status.is_success() || !body.valid {
            return Err(body.error.unwrap_or_else(|| "Invalid license key".into()));
        }

        // Cache the license locally
        let tenant = body.tenant.as_ref().ok_or("Missing tenant in response")?;
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let id = Uuid::new_v4().to_string();
        let modules_json = serde_json::to_string(&body.modules).unwrap_or_default();
        let cached = serde_json::to_string(&body).unwrap_or_default();

        sqlx::query(
            "INSERT INTO licenses \
             (id, license_key, tenant_id, tenant_name, tier, hardware_fingerprint, \
              allowed_modules, max_users, issued_at, expires_at, last_validated_at, \
              is_active, cached_response) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
        )
        .bind(&id)
        .bind(license_key)
        .bind(&tenant.id)
        .bind(&tenant.name)
        .bind(body.tier.as_deref().unwrap_or("starter"))
        .bind(&fingerprint)
        .bind(&modules_json)
        .bind(body.max_users.unwrap_or(5))
        .bind(&now)
        .bind(body.expires_at.as_deref().unwrap_or(&now))
        .bind(&now)
        .bind(&cached)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to cache license: {}", e))?;

        tracing::info!(
            "License activated for tenant '{}' (tier: {})",
            tenant.name,
            body.tier.as_deref().unwrap_or("starter")
        );

        Ok(body)
    }

    // The caching is handled inside the cfg blocks
    // Ok is also returned from within the cfg blocks
}

/// Validate an existing cached license against the cloud server.
///
/// Called on app startup. Falls back to cached license with grace period
/// if the server is unreachable.
pub async fn validate_license(pool: &SqlitePool) -> Result<LicenseStatus, String> {
    let license = get_cached_license(pool)
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    let license = match license {
        Some(l) => l,
        None => {
            return Ok(LicenseStatus {
                is_licensed: false,
                tenant_name: None,
                tier: None,
                expires_at: None,
                allowed_modules: vec![],
                max_users: 0,
            });
        }
    };

    if !license.is_active {
        return Err("License has been deactivated".into());
    }

    // Try online validation
    let fingerprint = get_hardware_fingerprint();
    let url = format!("{}/api/v1/license/validate", license_server_url());

    let client = reqwest::Client::new();
    let result = client
        .post(&url)
        .json(&serde_json::json!({
            "license_key": license.license_key,
            "hardware_fingerprint": fingerprint,
            "app_version": env!("CARGO_PKG_VERSION"),
        }))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    match result {
        Ok(response) => {
            if let Ok(body) = response.json::<LicenseServerResponse>().await {
                if body.valid {
                    // Update last_validated_at
                    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                    let _ = sqlx::query(
                        "UPDATE licenses SET last_validated_at = ? WHERE id = ?",
                    )
                    .bind(&now)
                    .bind(&license.id)
                    .execute(pool)
                    .await;

                    let modules: Vec<String> = serde_json::from_str(&license.allowed_modules)
                        .unwrap_or_default();

                    return Ok(LicenseStatus {
                        is_licensed: true,
                        tenant_name: Some(license.tenant_name),
                        tier: Some(license.tier),
                        expires_at: Some(license.expires_at),
                        allowed_modules: modules,
                        max_users: license.max_users,
                    });
                } else {
                    // License revoked/expired on server
                    let _ = sqlx::query(
                        "UPDATE licenses SET is_active = 0 WHERE id = ?",
                    )
                    .bind(&license.id)
                    .execute(pool)
                    .await;
                    return Err(body.error.unwrap_or_else(|| "License is no longer valid".into()));
                }
            }
            // Fall through to offline check if parsing failed
        }
        Err(e) => {
            tracing::warn!("License server unreachable: {}. Checking grace period.", e);
        }
    }

    // Offline fallback — check grace period
    check_grace_period(&license)
}

/// Check if the cached license is within the offline grace period.
fn check_grace_period(license: &License) -> Result<LicenseStatus, String> {
    let last_validated = NaiveDateTime::parse_from_str(
        &license.last_validated_at,
        "%Y-%m-%dT%H:%M:%SZ",
    )
    .map_err(|_| "Invalid last_validated_at timestamp")?;

    let now = Utc::now().naive_utc();
    let days_since = (now - last_validated).num_days();

    if days_since > GRACE_PERIOD_DAYS {
        return Err(format!(
            "License validation expired. Last validated {} days ago (grace period: {} days). \
             Please connect to the internet to re-validate.",
            days_since, GRACE_PERIOD_DAYS
        ));
    }

    tracing::info!(
        "Offline grace period active ({}/{} days used)",
        days_since,
        GRACE_PERIOD_DAYS
    );

    let modules: Vec<String> = serde_json::from_str(&license.allowed_modules)
        .unwrap_or_default();

    Ok(LicenseStatus {
        is_licensed: true,
        tenant_name: Some(license.tenant_name.clone()),
        tier: Some(license.tier.clone()),
        expires_at: Some(license.expires_at.clone()),
        allowed_modules: modules,
        max_users: license.max_users,
    })
}

/// Get the cached license from SQLite.
pub async fn get_cached_license(pool: &SqlitePool) -> Result<Option<License>, sqlx::Error> {
    let license = sqlx::query_as::<_, License>(
        "SELECT * FROM licenses WHERE is_active = 1 ORDER BY last_validated_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(license)
}
