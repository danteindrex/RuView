//! Tauri commands for role and permission management.

use crate::auth::{
    audit::{self, AuditAction},
    jwt,
    models::*,
    rls,
};
use crate::commands::auth::AuthManager;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

// ─── List Roles ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_roles(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Vec<Role>, String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    let roles = if let Some(tid) = rls::tenant_filter(&claims) {
        sqlx::query_as::<_, Role>(
            "SELECT * FROM roles WHERE tenant_id = ? ORDER BY name",
        )
        .bind(tid)
        .fetch_all(&auth.pool)
        .await
    } else {
        sqlx::query_as::<_, Role>("SELECT * FROM roles ORDER BY name")
            .fetch_all(&auth.pool)
            .await
    }
    .map_err(|e| format!("Query error: {}", e))?;

    Ok(roles)
}

// ─── Get Role with Modules ──────────────────────────────────────────────────

#[tauri::command]
pub async fn get_role(
    access_token: String,
    role_id: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<RoleWithModules, String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    let role = sqlx::query_as::<_, Role>("SELECT * FROM roles WHERE id = ?")
        .bind(&role_id)
        .fetch_optional(&auth.pool)
        .await
        .map_err(|e| format!("Query error: {}", e))?
        .ok_or("Role not found")?;

    // Tenant check
    if let Some(tid) = rls::tenant_filter(&claims) {
        if role.tenant_id != tid {
            return Err("Role not found".into());
        }
    }

    // Load modules with permissions
    let rows = sqlx::query_as::<_, RoleModule>(
        "SELECT * FROM role_modules WHERE role_id = ?",
    )
    .bind(&role_id)
    .fetch_all(&auth.pool)
    .await
    .map_err(|e| format!("Query error: {}", e))?;

    let mut modules = Vec::new();
    for rm in rows {
        let module = sqlx::query_as::<_, AccessModule>(
            "SELECT * FROM access_modules WHERE id = ?",
        )
        .bind(&rm.module_id)
        .fetch_one(&auth.pool)
        .await
        .map_err(|e| format!("Module load error: {}", e))?;

        let perm = sqlx::query_as::<_, Permission>(
            "SELECT * FROM permissions WHERE id = ?",
        )
        .bind(&rm.permission_id)
        .fetch_one(&auth.pool)
        .await
        .map_err(|e| format!("Permission load error: {}", e))?;

        modules.push(ModuleWithPermission {
            id: module.id,
            name: module.name,
            code: module.code,
            permission: PermissionDetail::from(perm),
        });
    }

    Ok(RoleWithModules {
        id: role.id,
        name: role.name,
        description: role.description,
        modules,
    })
}

// ─── Create Role ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_role(
    access_token: String,
    request: CreateRoleRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Role, String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    rls::check_module_access(&auth.pool, &claims, "user-management", "add")
        .await
        .map_err(|e| format!("Access check error: {}", e))?
        .then_some(())
        .ok_or("Insufficient permissions")?;

    let tenant_id = if claims.scope == "global" {
        let tid: Option<String> = sqlx::query_scalar("SELECT id FROM tenants LIMIT 1")
            .fetch_optional(&auth.pool)
            .await
            .map_err(|e| format!("Database error: {}", e))?;
        tid.ok_or("No tenant exists")?
    } else {
        claims.tenant_id.clone()
    };

    let role_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO roles (id, name, description, tenant_id) VALUES (?, ?, ?, ?)",
    )
    .bind(&role_id)
    .bind(&request.name)
    .bind(request.description.as_deref().unwrap_or(""))
    .bind(&tenant_id)
    .execute(&auth.pool)
    .await
    .map_err(|e| format!("Failed to create role: {}", e))?;

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::RoleChange,
        Some(&format!("Created role: {}", request.name)),
    )
    .await;

    let role = sqlx::query_as::<_, Role>("SELECT * FROM roles WHERE id = ?")
        .bind(&role_id)
        .fetch_one(&auth.pool)
        .await
        .map_err(|e| format!("Failed to read role: {}", e))?;

    Ok(role)
}

// ─── Delete Role ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn delete_role(
    access_token: String,
    role_id: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    rls::check_module_access(&auth.pool, &claims, "user-management", "delete")
        .await
        .map_err(|e| format!("Access check error: {}", e))?
        .then_some(())
        .ok_or("Insufficient permissions")?;

    // Check if any users have this role
    let user_count: i32 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_roles WHERE role_id = ?",
    )
    .bind(&role_id)
    .fetch_one(&auth.pool)
    .await
    .map_err(|e| format!("Count error: {}", e))?;

    if user_count > 0 {
        return Err(format!(
            "Cannot delete role: {} user(s) are assigned to it",
            user_count
        ));
    }

    // Delete related role_modules and permissions
    let perm_ids: Vec<String> = sqlx::query_scalar(
        "SELECT permission_id FROM role_modules WHERE role_id = ?",
    )
    .bind(&role_id)
    .fetch_all(&auth.pool)
    .await
    .map_err(|e| format!("Query error: {}", e))?;

    sqlx::query("DELETE FROM role_modules WHERE role_id = ?")
        .bind(&role_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Delete role_modules error: {}", e))?;

    for pid in &perm_ids {
        sqlx::query("DELETE FROM permissions WHERE id = ?")
            .bind(pid)
            .execute(&auth.pool)
            .await
            .map_err(|e| format!("Delete permission error: {}", e))?;
    }

    sqlx::query("DELETE FROM roles WHERE id = ?")
        .bind(&role_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Delete role error: {}", e))?;

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::RoleChange,
        Some(&format!("Deleted role: {}", role_id)),
    )
    .await;

    Ok(())
}

// ─── Set Role Permissions ───────────────────────────────────────────────────

#[tauri::command]
pub async fn set_role_permissions(
    access_token: String,
    request: SetRolePermissionsRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    rls::check_module_access(&auth.pool, &claims, "user-management", "edit")
        .await
        .map_err(|e| format!("Access check error: {}", e))?
        .then_some(())
        .ok_or("Insufficient permissions")?;

    // Delete existing role_modules and their permissions
    let old_perm_ids: Vec<String> = sqlx::query_scalar(
        "SELECT permission_id FROM role_modules WHERE role_id = ?",
    )
    .bind(&request.role_id)
    .fetch_all(&auth.pool)
    .await
    .map_err(|e| format!("Query error: {}", e))?;

    sqlx::query("DELETE FROM role_modules WHERE role_id = ?")
        .bind(&request.role_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Clear error: {}", e))?;

    for pid in &old_perm_ids {
        sqlx::query("DELETE FROM permissions WHERE id = ?")
            .bind(pid)
            .execute(&auth.pool)
            .await
            .map_err(|e| format!("Delete error: {}", e))?;
    }

    // Insert new permissions
    for mp in &request.module_permissions {
        let perm_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO permissions \
             (id, can_read, can_add, can_edit, can_delete, can_approve, \
              read_scope, add_scope, edit_scope, delete_scope, approve_scope) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&perm_id)
        .bind(mp.can_read)
        .bind(mp.can_add)
        .bind(mp.can_edit)
        .bind(mp.can_delete)
        .bind(mp.can_approve)
        .bind(mp.read_scope)
        .bind(mp.add_scope)
        .bind(mp.edit_scope)
        .bind(mp.delete_scope)
        .bind(mp.approve_scope)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Insert permission error: {}", e))?;

        let rm_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO role_modules (id, role_id, module_id, permission_id) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(&rm_id)
        .bind(&request.role_id)
        .bind(&mp.module_id)
        .bind(&perm_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Insert role_module error: {}", e))?;
    }

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::PermissionChange,
        Some(&format!(
            "Updated permissions for role {} ({} modules)",
            request.role_id,
            request.module_permissions.len()
        )),
    )
    .await;

    Ok(())
}

// ─── List Access Modules ────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_modules(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Vec<AccessModule>, String> {
    let _claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    let modules = sqlx::query_as::<_, AccessModule>(
        "SELECT * FROM access_modules ORDER BY name",
    )
    .fetch_all(&auth.pool)
    .await
    .map_err(|e| format!("Query error: {}", e))?;

    Ok(modules)
}

// ─── List Tenant Modules (for current user's tenant) ────────────────────────

#[tauri::command]
pub async fn list_tenant_modules(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Vec<AccessModule>, String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    if claims.scope == "global" {
        // Super admin sees all modules
        return list_modules(access_token, auth).await;
    }

    let modules = sqlx::query_as::<_, AccessModule>(
        "SELECT am.* FROM access_modules am \
         JOIN tenant_modules tm ON am.id = tm.module_id \
         WHERE tm.tenant_id = ? AND tm.is_active = 1 \
         ORDER BY am.name",
    )
    .bind(&claims.tenant_id)
    .fetch_all(&auth.pool)
    .await
    .map_err(|e| format!("Query error: {}", e))?;

    Ok(modules)
}
