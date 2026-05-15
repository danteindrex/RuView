//! Tauri commands for authentication, login, logout, and session management.
//!
//! Implements the dual-auth flow:
//! - Local users authenticate against embedded SQLite
//! - Super Admins authenticate against the cloud License Server

use crate::auth::{
    audit::{self, AuditAction},
    jwt,
    models::*,
    password,
    rate_limiter::RateLimiter,
    super_admin,
};
use sqlx::SqlitePool;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

/// Auth state managed by Tauri.
pub struct AuthManager {
    pub pool: SqlitePool,
    pub jwt_secret: String,
    pub login_limiter: RateLimiter,
}

// ─── Login (dual path) ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn login(
    email: String,
    password_input: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<serde_json::Value, String> {
    // Rate limit check
    if let Err(wait_secs) = auth.login_limiter.check(&email) {
        return Err(format!(
            "Too many login attempts. Please try again in {} seconds.",
            wait_secs
        ));
    }

    // Detect vendor login
    if super_admin::is_vendor_login(&email) {
        // Cloud auth path — authenticate against License Server
        let cached_license = crate::auth::license::get_cached_license(&auth.pool)
            .await
            .map_err(|e| format!("Database error: {}", e))?;

        let license_key = cached_license.map(|l| l.license_key);
        let response = super_admin::super_admin_login(
            &email,
            &password_input,
            license_key.as_deref(),
        )
        .await?;

        // Log vendor access
        let _ = audit::log(
            &auth.pool,
            Some(&response.user.id),
            AuditAction::VendorLogin,
            Some(&format!("Vendor {} logged in", email)),
        )
        .await;

        auth.login_limiter.clear(&email);

        // Re-sign the vendor claims into a local JWT so verify_access_token()
        // works with the local secret for all subsequent API calls.
        let now = chrono::Utc::now().timestamp();
        let local_claims = jwt::AccessClaims {
            sub: response.user.id.clone(),
            email: response.user.email.clone(),
            first_name: response.user.first_name.clone(),
            last_name: response.user.last_name.clone(),
            tenant_id: "global".into(),
            scope: "global".into(),
            iat: now,
            exp: now + 15 * 60, // 15 min
        };
        let local_token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &local_claims,
            &jsonwebtoken::EncodingKey::from_secret(auth.jwt_secret.as_bytes()),
        )
        .map_err(|e| format!("Token signing error: {}", e))?;

        return Ok(serde_json::json!({
            "access_token": local_token,
            "user": {
                "id": response.user.id,
                "email": response.user.email,
                "first_name": response.user.first_name,
                "last_name": response.user.last_name,
                "role": "super_admin",
                "scope": "global",
                "is_active": true,
                "must_change_password": false,
                "tenant_id": "global",
                "roles": [{
                    "id": "super-admin",
                    "name": "Super Admin",
                    "description": "Full vendor access",
                    "modules": []
                }]
            }
        }));
    }

    // Local auth path — authenticate against SQLite
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE email = ? AND is_active = 1",
    )
    .bind(&email)
    .fetch_optional(&auth.pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?;

    let user = match user {
        Some(u) => u,
        None => {
            let _ = audit::log(
                &auth.pool,
                None,
                AuditAction::LoginFailed,
                Some(&format!("Unknown email: {}", email)),
            )
            .await;
            return Err("Invalid email or password".into());
        }
    };

    // Verify password
    let valid = password::verify_password(&password_input, &user.password_hash)
        .map_err(|_| "Password verification error")?;

    if !valid {
        let _ = audit::log(
            &auth.pool,
            Some(&user.id),
            AuditAction::LoginFailed,
            Some("Invalid password"),
        )
        .await;
        return Err("Invalid email or password".into());
    }

    // Generate tokens
    let access_token = jwt::generate_access_token(
        &user.id,
        &user.email,
        &user.first_name,
        &user.last_name,
        &user.tenant_id,
        &auth.jwt_secret,
    )
    .map_err(|e| format!("Token generation error: {}", e))?;

    let refresh_token_str = jwt::generate_refresh_token();
    let refresh_expiry = jwt::refresh_token_expiry();
    let rt_id = Uuid::new_v4().to_string();

    // Store refresh token
    sqlx::query(
        "INSERT INTO refresh_tokens (id, token, user_id, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&rt_id)
    .bind(&refresh_token_str)
    .bind(&user.id)
    .bind(&refresh_expiry)
    .execute(&auth.pool)
    .await
    .map_err(|e| format!("Failed to store refresh token: {}", e))?;

    // Update last_login
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let _ = sqlx::query("UPDATE users SET last_login = ? WHERE id = ?")
        .bind(&now)
        .bind(&user.id)
        .execute(&auth.pool)
        .await;

    // Get user's roles with modules and permissions
    let roles = load_user_roles(&auth.pool, &user.id).await?;

    // Audit log
    let _ = audit::log(
        &auth.pool,
        Some(&user.id),
        AuditAction::Login,
        None,
    )
    .await;

    auth.login_limiter.clear(&email);

    let mut user_response = UserResponse::from(user);
    user_response.roles = roles;

    Ok(serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token_str,
        "user": user_response,
    }))
}

// ─── Logout ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn logout(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    // Try to decode the token to get the user ID for audit logging
    if let Ok(claims) = jwt::verify_access_token(&access_token, &auth.jwt_secret) {
        // Revoke all refresh tokens for this user
        let _ = sqlx::query(
            "UPDATE refresh_tokens SET revoked = 1, revoked_at = datetime('now') \
             WHERE user_id = ? AND revoked = 0",
        )
        .bind(&claims.sub)
        .execute(&auth.pool)
        .await;

        let _ = audit::log(
            &auth.pool,
            Some(&claims.sub),
            AuditAction::Logout,
            None,
        )
        .await;
    }

    Ok(())
}

// ─── Refresh Token ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn refresh_token(
    refresh_token_str: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<serde_json::Value, String> {
    // Find the refresh token
    let rt = sqlx::query_as::<_, RefreshToken>(
        "SELECT * FROM refresh_tokens WHERE token = ? AND revoked = 0",
    )
    .bind(&refresh_token_str)
    .fetch_optional(&auth.pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?;

    let rt = match rt {
        Some(t) => t,
        None => return Err("Invalid or revoked refresh token".into()),
    };

    // Check expiry
    let expires = chrono::NaiveDateTime::parse_from_str(&rt.expires_at, "%Y-%m-%dT%H:%M:%SZ")
        .map_err(|_| "Invalid expiry format")?;
    if chrono::Utc::now().naive_utc() > expires {
        return Err("Refresh token expired".into());
    }

    // Load the user
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ? AND is_active = 1")
        .bind(&rt.user_id)
        .fetch_optional(&auth.pool)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or("User not found or deactivated")?;

    // Rotate: revoke old token, create new one
    let new_refresh = jwt::generate_refresh_token();
    let new_expiry = jwt::refresh_token_expiry();
    let new_rt_id = Uuid::new_v4().to_string();

    sqlx::query(
        "UPDATE refresh_tokens SET revoked = 1, revoked_at = datetime('now'), \
         replaced_by_token = ? WHERE id = ?",
    )
    .bind(&new_refresh)
    .bind(&rt.id)
    .execute(&auth.pool)
    .await
    .map_err(|e| format!("Failed to revoke old token: {}", e))?;

    sqlx::query(
        "INSERT INTO refresh_tokens (id, token, user_id, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&new_rt_id)
    .bind(&new_refresh)
    .bind(&user.id)
    .bind(&new_expiry)
    .execute(&auth.pool)
    .await
    .map_err(|e| format!("Failed to store new refresh token: {}", e))?;

    // Generate new access token
    let new_access = jwt::generate_access_token(
        &user.id,
        &user.email,
        &user.first_name,
        &user.last_name,
        &user.tenant_id,
        &auth.jwt_secret,
    )
    .map_err(|e| format!("Token generation error: {}", e))?;

    Ok(serde_json::json!({
        "access_token": new_access,
        "refresh_token": new_refresh,
    }))
}

// ─── Current User ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_current_user(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<UserResponse, String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    if claims.scope == "global" {
        // Super admin — return synthetic user
        return Ok(UserResponse {
            id: claims.sub,
            first_name: claims.first_name,
            last_name: claims.last_name,
            email: claims.email,
            phone: None,
            avatar_url: None,
            is_active: true,
            must_change_password: false,
            tenant_id: "global".into(),
            department_id: None,
            last_login: None,
            roles: vec![RoleWithModules {
                id: "super-admin".into(),
                name: "Super Admin".into(),
                description: "Full vendor access".into(),
                modules: vec![],
            }],
            scope: Some("global".into()),
            created_at: String::new(),
        });
    }

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&claims.sub)
        .fetch_optional(&auth.pool)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or("User not found")?;

    let roles = load_user_roles(&auth.pool, &user.id).await?;
    let mut response = UserResponse::from(user);
    response.roles = roles;
    Ok(response)
}

// ─── Change Password ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn change_password(
    access_token: String,
    old_password: String,
    new_password: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    // Validate new password strength
    password::validate_password_strength(&new_password)?;

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&claims.sub)
        .fetch_optional(&auth.pool)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or("User not found")?;

    // Verify old password
    let valid = password::verify_password(&old_password, &user.password_hash)
        .map_err(|_| "Password verification error")?;
    if !valid {
        return Err("Current password is incorrect".into());
    }

    // Hash new password
    let new_hash = password::hash_password(&new_password)
        .map_err(|_| "Failed to hash new password")?;

    sqlx::query(
        "UPDATE users SET password_hash = ?, must_change_password = 0, \
         updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&new_hash)
    .bind(&claims.sub)
    .execute(&auth.pool)
    .await
    .map_err(|e| format!("Failed to update password: {}", e))?;

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::PasswordChange,
        None,
    )
    .await;

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Load a user's roles with their modules and permissions.
async fn load_user_roles(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<RoleWithModules>, String> {
    // Get all roles for this user
    let roles = sqlx::query_as::<_, Role>(
        "SELECT r.* FROM roles r \
         JOIN user_roles ur ON r.id = ur.role_id \
         WHERE ur.user_id = ?",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load roles: {}", e))?;

    let mut result = Vec::new();

    for role in roles {
        // Get modules with permissions for this role
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT am.id, am.name, am.code, p.id as perm_id \
             FROM role_modules rm \
             JOIN access_modules am ON rm.module_id = am.id \
             JOIN permissions p ON rm.permission_id = p.id \
             WHERE rm.role_id = ?",
        )
        .bind(&role.id)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Failed to load role modules: {}", e))?;

        let mut modules = Vec::new();
        for (mod_id, mod_name, mod_code, perm_id) in rows {
            let perm = sqlx::query_as::<_, Permission>(
                "SELECT * FROM permissions WHERE id = ?",
            )
            .bind(&perm_id)
            .fetch_one(pool)
            .await
            .map_err(|e| format!("Failed to load permission: {}", e))?;

            modules.push(ModuleWithPermission {
                id: mod_id,
                name: mod_name,
                code: mod_code,
                permission: PermissionDetail::from(perm),
            });
        }

        result.push(RoleWithModules {
            id: role.id,
            name: role.name,
            description: role.description,
            modules,
        });
    }

    Ok(result)
}
