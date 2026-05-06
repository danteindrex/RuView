//! Row-Level Security (RLS) enforcement.
//!
//! Since SQLite doesn't support database-level RLS policies, we enforce
//! tenant isolation at the query level. All tenant-scoped queries go through
//! these helpers which inject `WHERE tenant_id = ?` filtering.
//!
//! Super Admin users (scope = "global") bypass tenant filtering.

use crate::auth::jwt::AccessClaims;

/// Whether the current user should have tenant-scoped queries.
///
/// Returns `None` for super admins (no filter), or `Some(tenant_id)` for
/// regular users.
pub fn tenant_filter(claims: &AccessClaims) -> Option<&str> {
    if claims.scope == "global" {
        None // Super admin — no tenant filtering
    } else {
        Some(&claims.tenant_id)
    }
}

/// Build a WHERE clause fragment for tenant-scoped queries.
///
/// Returns `("AND tenant_id = ?", Some(tenant_id))` for local users,
/// or `("", None)` for super admins.
pub fn tenant_where_clause(claims: &AccessClaims) -> (&str, Option<&str>) {
    if claims.scope == "global" {
        ("", None)
    } else {
        ("AND tenant_id = ?", Some(&claims.tenant_id))
    }
}

/// Check if the user has permission to access a specific module.
///
/// This mirrors Taha's `checkUserAccess` from `auth.service.ts`:
/// 1. Check if the module is active for the user's tenant (tenant_modules)
/// 2. Check if the user's role has the required permission on that module
///
/// Super admins always return `true`.
pub async fn check_module_access(
    pool: &sqlx::SqlitePool,
    claims: &AccessClaims,
    module_code: &str,
    permission_type: &str,
) -> Result<bool, sqlx::Error> {
    // Super admins bypass all checks
    if claims.scope == "global" {
        return Ok(true);
    }

    // 1. Check if the module is active for the user's tenant
    let tenant_has_module: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM tenant_modules tm \
         JOIN access_modules am ON tm.module_id = am.id \
         WHERE tm.tenant_id = ? AND am.code = ? AND tm.is_active = 1",
    )
    .bind(&claims.tenant_id)
    .bind(module_code)
    .fetch_one(pool)
    .await?;

    if !tenant_has_module {
        return Ok(false);
    }

    // 2. Check user's role has the required permission
    let permission_column = match permission_type {
        "view" | "read" | "get" => "can_read",
        "add" | "post" => "can_add",
        "edit" | "put" | "patch" => "can_edit",
        "delete" => "can_delete",
        "approve" => "can_approve",
        _ => return Ok(false),
    };

    let query = format!(
        "SELECT COUNT(*) > 0 FROM user_roles ur \
         JOIN role_modules rm ON ur.role_id = rm.role_id \
         JOIN access_modules am ON rm.module_id = am.id \
         JOIN permissions p ON rm.permission_id = p.id \
         WHERE ur.user_id = ? AND am.code = ? AND p.{} = 1",
        permission_column
    );

    let has_permission: bool = sqlx::query_scalar(&query)
        .bind(&claims.sub)
        .bind(module_code)
        .fetch_one(pool)
        .await?;

    Ok(has_permission)
}
