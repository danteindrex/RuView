//! Authentication guard for protecting Tauri commands.
//!
//! Provides a `require_auth()` helper that legacy commands call to
//! enforce that the caller has a valid access token before proceeding.

use crate::auth::jwt;
use crate::commands::auth::AuthManager;
use std::sync::Arc;


/// Verify that the caller has a valid access token.
///
/// This is the backend enforcement point — even if the frontend login
/// screen is bypassed (e.g., via JavaScript injection), this function
/// ensures every Tauri command requires authentication.
///
/// Returns `Ok(claims)` on success, `Err(String)` on failure.
pub fn require_auth(
    access_token: &str,
    auth: &Arc<AuthManager>,
) -> Result<jwt::AccessClaims, String> {
    jwt::verify_access_token(access_token, &auth.jwt_secret)
        .map_err(|_| "Authentication required. Please log in.".to_string())
}

/// Check that the user has a specific module permission.
/// Combines auth + permission check in one call.
pub async fn require_permission(
    access_token: &str,
    auth: &Arc<AuthManager>,
    module_code: &str,
    permission_type: &str,
) -> Result<jwt::AccessClaims, String> {
    let claims = require_auth(access_token, auth)?;

    let has_access = crate::auth::rls::check_module_access(
        &auth.pool, &claims, module_code, permission_type,
    )
    .await
    .map_err(|e| format!("Permission check failed: {}", e))?;

    if !has_access {
        return Err(format!(
            "Insufficient permissions: {} requires '{}' access",
            module_code, permission_type
        ));
    }

    Ok(claims)
}
