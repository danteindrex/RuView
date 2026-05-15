//! Cloud-authenticated Super Admin login.
//!
//! Super Admins are vendor team members whose credentials live on the
//! cloud License Server. When they log in from any customer's desktop,
//! the desktop sends their credentials to the License Server for validation.
//!
//! The returned JWT has `scope: "global"` which bypasses tenant filtering.

use crate::auth::license::get_hardware_fingerprint;
use serde::{Deserialize, Serialize};

/// Default license server URL.
const DEFAULT_LICENSE_SERVER_URL: &str = "https://license.wave.io";

/// Response from the super-admin login endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct SuperLoginResponse {
    pub token: String,
    pub user: SuperAdminUser,
    pub permissions: String, // "*" = all
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuperAdminUser {
    pub id: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub role: String,  // "super_admin"
    pub scope: String, // "global"
}

/// Detect if a login attempt should be routed to the cloud License Server.
///
/// Returns `true` if:
/// - The email domain is `@wave.io` (configurable), OR
/// - The `is_vendor_login` flag is explicitly set
pub fn is_vendor_login(email: &str) -> bool {
    let vendor_domain = std::env::var("WAVE_VENDOR_DOMAIN")
        .unwrap_or_else(|_| "wave.io".to_string());

    email.ends_with(&format!("@{}", vendor_domain))
}

/// Authenticate a Super Admin against the cloud License Server.
///
/// This is the cloud auth path. The desktop sends credentials to:
/// `POST /api/v1/auth/super-login`
///
/// On success, the License Server returns a JWT with `scope: "global"`.
pub async fn super_admin_login(
    email: &str,
    password: &str,
    license_key: Option<&str>,
) -> Result<SuperLoginResponse, String> {
    // ─── Debug Bypass ───────────────────────────────────────────────────────
    #[cfg(debug_assertions)]
    {
        if email == "admin@wave.io" && password == "admin" {
            tracing::info!("Bypassing cloud auth for Super Admin (Debug Mode)");
            return Ok(SuperLoginResponse {
                token: "debug-super-token".into(),
                user: SuperAdminUser {
                    id: "super-admin-uuid".into(),
                    email: "admin@wave.io".into(),
                    first_name: "Vendor".into(),
                    last_name: "Admin".into(),
                    role: "super_admin".into(),
                    scope: "global".into(),
                },
                permissions: "*".into(),
            });
        }
    }

    let fingerprint = get_hardware_fingerprint();
    let base_url = std::env::var("WAVE_LICENSE_SERVER_URL")
        .unwrap_or_else(|_| DEFAULT_LICENSE_SERVER_URL.to_string());
    let url = format!("{}/api/v1/auth/super-login", base_url);

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "email": email,
            "password": password,
            "hardware_fingerprint": fingerprint,
            "license_key": license_key,
        }))
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| {
            format!(
                "Cannot reach license server for vendor authentication: {}. \
                 Super Admin login requires an internet connection.",
                e
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body: serde_json::Value = response
            .json()
            .await
            .unwrap_or(serde_json::json!({"message": "Authentication failed"}));
        let msg = body["message"]
            .as_str()
            .unwrap_or("Invalid vendor credentials");
        return Err(msg.to_string());
    }

    let login_response: SuperLoginResponse = response
        .json()
        .await
        .map_err(|e| format!("Invalid response from license server: {}", e))?;

    tracing::info!(
        "Super Admin '{}' authenticated via cloud",
        login_response.user.email
    );

    Ok(login_response)
}

/// Verify a Super Admin JWT is still valid by checking with the cloud.
///
/// Called periodically (e.g., every 15 minutes) to ensure the vendor's
/// session hasn't been revoked.
pub async fn verify_super_admin_token(token: &str) -> Result<bool, String> {
    let base_url = std::env::var("WAVE_LICENSE_SERVER_URL")
        .unwrap_or_else(|_| DEFAULT_LICENSE_SERVER_URL.to_string());
    let url = format!("{}/api/v1/auth/super-verify", base_url);

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .bearer_auth(token)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Cannot verify vendor token: {}", e))?;

    Ok(response.status().is_success())
}
