//! Tauri commands for user management (CRUD + role assignment).

use crate::auth::{
    audit::{self, AuditAction},
    jwt,
    models::*,
    password,
    rls,
};
use crate::commands::auth::AuthManager;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

// ─── List Users ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_users(
    access_token: String,
    page: Option<i32>,
    limit: Option<i32>,
    search: Option<String>,
    tenant_id: Option<String>,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<serde_json::Value, String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    // Permission check
    rls::check_module_access(&auth.pool, &claims, "user-management", "view")
        .await
        .map_err(|e| format!("Access check error: {}", e))?
        .then_some(())
        .ok_or("Insufficient permissions")?;

    let page = page.unwrap_or(0);
    let limit = limit.unwrap_or(25).min(100);
    let offset = page * limit;

    let tenant_filter = rls::tenant_filter(&claims);

    let (users, total): (Vec<User>, i32) = if let Some(tid) = tenant_filter {
        let search_pattern = search
            .as_deref()
            .map(|s| format!("%{}%", s))
            .unwrap_or_else(|| "%".into());

        let users = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE tenant_id = ? AND \
             (first_name LIKE ? OR last_name LIKE ? OR email LIKE ?) \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(tid)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(&auth.pool)
        .await
        .map_err(|e| format!("Query error: {}", e))?;

        let total: i32 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE tenant_id = ? AND \
             (first_name LIKE ? OR last_name LIKE ? OR email LIKE ?)",
        )
        .bind(tid)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .bind(&search_pattern)
        .fetch_one(&auth.pool)
        .await
        .map_err(|e| format!("Count error: {}", e))?;

        (users, total)
    } else {
        // Super admin — check if filtering by tenant
        let search_pattern = search
            .as_deref()
            .map(|s| format!("%{}%", s))
            .unwrap_or_else(|| "%".into());

        if let Some(tid) = &tenant_id {
            let users = sqlx::query_as::<_, User>(
                "SELECT * FROM users WHERE tenant_id = ? AND \
                 (first_name LIKE ? OR last_name LIKE ? OR email LIKE ?) \
                 ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(tid)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(&auth.pool)
            .await
            .map_err(|e| format!("Query error: {}", e))?;

            let total: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM users WHERE tenant_id = ? AND \
                 (first_name LIKE ? OR last_name LIKE ? OR email LIKE ?)",
            )
            .bind(tid)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_one(&auth.pool)
            .await
            .map_err(|e| format!("Count error: {}", e))?;

            (users, total)
        } else {
            let users = sqlx::query_as::<_, User>(
                "SELECT * FROM users WHERE \
                 (first_name LIKE ? OR last_name LIKE ? OR email LIKE ?) \
                 ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(&auth.pool)
            .await
            .map_err(|e| format!("Query error: {}", e))?;

            let total: i32 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM users WHERE \
                 (first_name LIKE ? OR last_name LIKE ? OR email LIKE ?)",
            )
            .bind(&search_pattern)
            .bind(&search_pattern)
            .bind(&search_pattern)
            .fetch_one(&auth.pool)
            .await
            .map_err(|e| format!("Count error: {}", e))?;

            (users, total)
        }
    };

    let mut responses: Vec<UserResponse> = users.into_iter().map(UserResponse::from).collect();
    let mut final_total = total;

    // If caller is a super admin, prepend them to the list so they "appear" as a user
    // Only if not filtering by a specific tenant, or if filtering by 'global'
    if claims.scope == "global" && (tenant_id.is_none() || tenant_id == Some("global".into())) {
        let super_user = UserResponse {
            id: format!("sa-{}", claims.sub),
            first_name: claims.first_name.clone(),
            last_name: claims.last_name.clone(),
            email: claims.email.clone(),
            phone: None,
            avatar_url: None,
            is_active: true,
            must_change_password: false,
            tenant_id: "global".into(),
            department_id: None,
            last_login: None, // We don't track this locally for SA
            roles: vec![RoleWithModules {
                id: "role-super-admin".into(),
                name: "Super Admin".into(),
                description: "Global system access".into(),
                modules: Vec::new(), // Frontend handles SA permissions via isSuperAdmin flag
            }],
            scope: Some("global".into()),
            created_at: "".into(),
        };
        responses.insert(0, super_user);
        final_total += 1;
    }

    Ok(serde_json::json!({
        "users": responses,
        "total": final_total,
        "page": page,
        "limit": limit,
    }))
}

// ─── Get User ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_user(
    access_token: String,
    user_id: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<UserResponse, String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    rls::check_module_access(&auth.pool, &claims, "user-management", "view")
        .await
        .map_err(|e| format!("Access check error: {}", e))?
        .then_some(())
        .ok_or("Insufficient permissions")?;

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&auth.pool)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or("User not found")?;

    // Tenant check (non-super-admin can only see own tenant's users)
    if let Some(tid) = rls::tenant_filter(&claims) {
        if user.tenant_id != tid {
            return Err("User not found".into());
        }
    }

    Ok(UserResponse::from(user))
}

// ─── Create User ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_user(
    access_token: String,
    request: CreateUserRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<UserResponse, String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    rls::check_module_access(&auth.pool, &claims, "user-management", "add")
        .await
        .map_err(|e| format!("Access check error: {}", e))?
        .then_some(())
        .ok_or("Insufficient permissions")?;

    // Validate password
    password::validate_password_strength(&request.password)?;

    // Check license user limit
    let tenant_id = if claims.scope == "global" {
        // Super admin can specify tenant_id in request
        if let Some(tid) = &request.tenant_id {
            tid.clone()
        } else {
            // Fallback to the first local tenant if not specified
            let tid: Option<String> = sqlx::query_scalar(
                "SELECT id FROM tenants LIMIT 1",
            )
            .fetch_optional(&auth.pool)
            .await
            .map_err(|e| format!("Database error: {}", e))?;
            tid.ok_or("No tenant exists")?
        }
    } else {
        claims.tenant_id.clone()
    };

    // Check max users limit
    let current_count: i32 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE tenant_id = ?",
    )
    .bind(&tenant_id)
    .fetch_one(&auth.pool)
    .await
    .map_err(|e| format!("Count error: {}", e))?;

    let max_users: Option<i32> = sqlx::query_scalar(
        "SELECT max_users FROM licenses WHERE tenant_id = ? AND is_active = 1",
    )
    .bind(&tenant_id)
    .fetch_optional(&auth.pool)
    .await
    .map_err(|e| format!("License check error: {}", e))?;

    if let Some(max) = max_users {
        if current_count >= max {
            return Err(format!(
                "User limit reached ({}/{}). Upgrade your license to add more users.",
                current_count, max
            ));
        }
    }

    // Check email uniqueness
    let exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM users WHERE email = ?",
    )
    .bind(&request.email)
    .fetch_one(&auth.pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?;

    if exists {
        return Err("A user with this email already exists".into());
    }

    // Hash password
    let hash = password::hash_password(&request.password)
        .map_err(|_| "Failed to hash password")?;

    let user_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO users (id, first_name, last_name, email, phone, password_hash, \
         is_active, must_change_password, tenant_id, department_id) \
         VALUES (?, ?, ?, ?, ?, ?, 1, 1, ?, ?)",
    )
    .bind(&user_id)
    .bind(&request.first_name)
    .bind(&request.last_name)
    .bind(&request.email)
    .bind(&request.phone)
    .bind(&hash)
    .bind(&tenant_id)
    .bind(&request.department_id)
    .execute(&auth.pool)
    .await
    .map_err(|e| format!("Failed to create user: {}", e))?;

    // Assign role if specified
    if let Some(role_id) = &request.role_id {
        let ur_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO user_roles (id, user_id, role_id) VALUES (?, ?, ?)",
        )
        .bind(&ur_id)
        .bind(&user_id)
        .bind(role_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Failed to assign role: {}", e))?;
    }

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::UserCreate,
        Some(&format!("Created user: {} ({})", request.email, user_id)),
    )
    .await;

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_one(&auth.pool)
        .await
        .map_err(|e| format!("Failed to read created user: {}", e))?;

    Ok(UserResponse::from(user))
}

// ─── Update User ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn update_user(
    access_token: String,
    user_id: String,
    request: UpdateUserRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<UserResponse, String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    rls::check_module_access(&auth.pool, &claims, "user-management", "edit")
        .await
        .map_err(|e| format!("Access check error: {}", e))?
        .then_some(())
        .ok_or("Insufficient permissions")?;

    // Verify the target user belongs to the caller's tenant (RLS)
    let target_user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&auth.pool)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or("User not found")?;

    if let Some(tid) = rls::tenant_filter(&claims) {
        if target_user.tenant_id != tid {
            return Err("User not found".into());
        }
    }

    // Build dynamic UPDATE
    let mut sets = Vec::new();
    let mut binds: Vec<String> = Vec::new();

    if let Some(v) = &request.first_name {
        sets.push("first_name = ?");
        binds.push(v.clone());
    }
    if let Some(v) = &request.last_name {
        sets.push("last_name = ?");
        binds.push(v.clone());
    }
    if let Some(v) = &request.email {
        sets.push("email = ?");
        binds.push(v.clone());
    }
    if let Some(v) = &request.phone {
        sets.push("phone = ?");
        binds.push(v.clone());
    }
    if let Some(v) = request.is_active {
        sets.push("is_active = ?");
        binds.push(if v { "1".into() } else { "0".into() });
    }

    if sets.is_empty() {
        return Err("No fields to update".into());
    }

    sets.push("updated_at = datetime('now')");

    let query = format!("UPDATE users SET {} WHERE id = ?", sets.join(", "));
    let mut q = sqlx::query(&query);
    for b in &binds {
        q = q.bind(b);
    }
    q = q.bind(&user_id);

    q.execute(&auth.pool)
        .await
        .map_err(|e| format!("Update failed: {}", e))?;

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::UserUpdate,
        Some(&format!("Updated user: {}", user_id)),
    )
    .await;

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_one(&auth.pool)
        .await
        .map_err(|e| format!("Failed to read updated user: {}", e))?;

    Ok(UserResponse::from(user))
}

// ─── Delete User ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn delete_user(
    access_token: String,
    user_id: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    rls::check_module_access(&auth.pool, &claims, "user-management", "delete")
        .await
        .map_err(|e| format!("Access check error: {}", e))?
        .then_some(())
        .ok_or("Insufficient permissions")?;

    // Cannot delete yourself
    if claims.sub == user_id {
        return Err("Cannot delete your own account".into());
    }

    // Verify the target user belongs to the caller's tenant (RLS)
    let target_user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&auth.pool)
        .await
        .map_err(|e| format!("Database error: {}", e))?
        .ok_or("User not found")?;

    if let Some(tid) = rls::tenant_filter(&claims) {
        if target_user.tenant_id != tid {
            return Err("User not found".into());
        }
    }

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&user_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Delete failed: {}", e))?;

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::UserDelete,
        Some(&format!("Deleted user: {}", user_id)),
    )
    .await;

    Ok(())
}

// ─── Assign Role ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn assign_user_role(
    access_token: String,
    user_id: String,
    role_id: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    let claims = jwt::verify_access_token(&access_token, &auth.jwt_secret)
        .map_err(|_| "Invalid or expired token")?;

    rls::check_module_access(&auth.pool, &claims, "user-management", "edit")
        .await
        .map_err(|e| format!("Access check error: {}", e))?
        .then_some(())
        .ok_or("Insufficient permissions")?;

    // Remove existing roles and assign new one
    sqlx::query("DELETE FROM user_roles WHERE user_id = ?")
        .bind(&user_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Failed to clear roles: {}", e))?;

    let ur_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO user_roles (id, user_id, role_id) VALUES (?, ?, ?)")
        .bind(&ur_id)
        .bind(&user_id)
        .bind(&role_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Failed to assign role: {}", e))?;

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::RoleChange,
        Some(&format!("Assigned role {} to user {}", role_id, user_id)),
    )
    .await;

    Ok(())
}
