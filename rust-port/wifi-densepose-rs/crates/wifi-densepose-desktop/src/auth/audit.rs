//! Audit logging for sensitive operations.
//!
//! Logs all authentication events, user management changes, and vendor access.
//! Super Admin actions are tagged with `[VENDOR]` prefix for traceability.

use sqlx::SqlitePool;
use uuid::Uuid;

/// Audit action types.
pub enum AuditAction {
    Login,
    LoginFailed,
    Logout,
    PasswordChange,
    UserCreate,
    UserUpdate,
    UserDelete,
    RoleChange,
    PermissionChange,
    LicenseActivate,
    LicenseValidate,
    VendorLogin,
    VendorAction(String),
}

impl AuditAction {
    /// Convert to storage string. Vendor actions get the `[VENDOR]` prefix.
    pub fn as_str(&self) -> String {
        match self {
            Self::Login => "login".into(),
            Self::LoginFailed => "login_failed".into(),
            Self::Logout => "logout".into(),
            Self::PasswordChange => "password_change".into(),
            Self::UserCreate => "user_create".into(),
            Self::UserUpdate => "user_update".into(),
            Self::UserDelete => "user_delete".into(),
            Self::RoleChange => "role_change".into(),
            Self::PermissionChange => "permission_change".into(),
            Self::LicenseActivate => "license_activate".into(),
            Self::LicenseValidate => "license_validate".into(),
            Self::VendorLogin => "[VENDOR] login".into(),
            Self::VendorAction(action) => format!("[VENDOR] {}", action),
        }
    }
}

/// Record an audit log entry.
pub async fn log(
    pool: &SqlitePool,
    user_id: Option<&str>,
    action: AuditAction,
    details: Option<&str>,
) -> Result<(), sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let action_str = action.as_str();

    sqlx::query(
        "INSERT INTO audit_logs (id, user_id, action, details) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&action_str)
    .bind(details)
    .execute(pool)
    .await?;

    tracing::info!(
        "Audit: {} (user={})",
        action_str,
        user_id.unwrap_or("system")
    );
    Ok(())
}

/// Retrieve recent audit logs (paginated).
pub async fn get_recent(
    pool: &SqlitePool,
    limit: i32,
    offset: i32,
) -> Result<Vec<crate::auth::models::AuditLog>, sqlx::Error> {
    let logs = sqlx::query_as::<_, crate::auth::models::AuditLog>(
        "SELECT * FROM audit_logs ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(logs)
}
