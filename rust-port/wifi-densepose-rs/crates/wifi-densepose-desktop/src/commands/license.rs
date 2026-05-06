//! Tauri commands for license management.

use crate::auth::{
    audit::{self, AuditAction},
    license, models::*, password, seed,
};
use crate::commands::auth::AuthManager;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

// ─── License Status ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn license_status(
    auth: State<'_, Arc<AuthManager>>,
) -> Result<LicenseStatus, String> {
    license::validate_license(&auth.pool).await
}

// ─── Activate License ───────────────────────────────────────────────────────

/// Activate a license key and set up the local tenant + first admin user.
///
/// This is the first-launch flow:
/// 1. Validate key with cloud License Server
/// 2. Create local tenant from server response
/// 3. Seed tenant_modules (allowed modules)
/// 4. Create default Admin role with full permissions
/// 5. Create the customer's first local admin user
#[tauri::command]
pub async fn activate_license(
    request: ActivateLicenseRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<LicenseStatus, String> {
    // Activate with the cloud server
    let response = license::activate_license(&auth.pool, &request.license_key).await?;

    let tenant_info = response
        .tenant
        .as_ref()
        .ok_or("License server did not return tenant info")?;

    // Create the local tenant
    sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name, industry, type, verification_status) \
         VALUES (?, ?, ?, 'COMPANY', 'verified')",
    )
    .bind(&tenant_info.id)
    .bind(&tenant_info.name)
    .bind(&tenant_info.industry)
    .execute(&auth.pool)
    .await
    .map_err(|e| format!("Failed to create tenant: {}", e))?;

    // Seed tenant modules from the license
    let modules = response.modules.as_deref().unwrap_or(&[]);
    seed::seed_tenant_modules(&auth.pool, &tenant_info.id, modules)
        .await
        .map_err(|e| format!("Failed to seed tenant modules: {}", e))?;

    // Create the default Admin role
    let role_id = seed::create_default_admin_role(
        &auth.pool,
        &tenant_info.id,
        modules,
    )
    .await
    .map_err(|e| format!("Failed to create admin role: {}", e))?;

    // Create the first tenant administrator (from request or fallback)
    let admin_info = request.admin_details.as_ref().ok_or("Admin details are required for first-time activation")?;
    
    let user_id = Uuid::new_v4().to_string();
    let hash = password::hash_password(&admin_info.password)
        .map_err(|_| "Failed to hash password")?;
    
    sqlx::query(
        "INSERT INTO users \
         (id, first_name, last_name, email, password_hash, is_active, \
          must_change_password, tenant_id) \
         VALUES (?, ?, ?, ?, ?, 1, 1, ?)",
    )
    .bind(&user_id)
    .bind(&admin_info.first_name)
    .bind(&admin_info.last_name)
    .bind(&admin_info.email)
    .bind(&hash)
    .bind(&tenant_info.id)
    .execute(&auth.pool)
    .await
    .map_err(|e| format!("Failed to create admin user: {}", e))?;

    // Assign the Admin role
    let ur_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO user_roles (id, user_id, role_id) VALUES (?, ?, ?)")
        .bind(&ur_id)
        .bind(&user_id)
        .bind(&role_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Failed to assign admin role: {}", e))?;

    let _ = audit::log(
        &auth.pool,
        Some(&user_id),
        AuditAction::LicenseActivate,
        Some(&format!(
            "License activated for tenant '{}'. Default admin seeded.",
            tenant_info.name
        )),
    )
    .await;

    tracing::info!(
        "License activated. Tenant: '{}', Default Admin Seeded",
        tenant_info.name
    );

    // Return license status
    license::validate_license(&auth.pool).await
}

// ─── Tenant Management ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_tenants(
    access_token: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Vec<Tenant>, String> {
    crate::commands::guard::require_auth(&access_token, &auth)?;
    
    let tenants = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants ORDER BY created_at DESC")
        .fetch_all(&auth.pool)
        .await
        .map_err(|e| format!("Database error: {}", e))?;
    Ok(tenants)
}

#[tauri::command]
pub async fn create_tenant(
    access_token: String,
    request: CreateTenantRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<Tenant, String> {
    let claims = crate::commands::guard::require_auth(&access_token, &auth)?;
    if claims.scope != "global" {
        return Err("Only super admins can create tenants".into());
    }

    let tenant_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO tenants (id, name, industry, domain, contact, email, location, currency_code, description) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&tenant_id)
    .bind(&request.name)
    .bind(request.industry.as_deref().unwrap_or("General"))
    .bind(&request.domain)
    .bind(&request.contact)
    .bind(&request.email)
    .bind(&request.location)
    .bind(request.currency_code.as_deref().unwrap_or("USD"))
    .bind(&request.description)
    .execute(&auth.pool)
    .await
    .map_err(|e| format!("Failed to create tenant: {}", e))?;

    let tenant = sqlx::query_as::<_, Tenant>("SELECT * FROM tenants WHERE id = ?")
        .bind(&tenant_id)
        .fetch_one(&auth.pool)
        .await
        .map_err(|e| format!("Failed to read created tenant: {}", e))?;

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::UserCreate, // Reuse or add AuditAction::TenantCreate if it exists
        Some(&format!("Created tenant: {} ({})", tenant.name, tenant.id)),
    )
    .await;

    Ok(tenant)
}

#[tauri::command]
pub async fn assign_tenant_modules(
    access_token: String,
    request: AssignTenantModulesRequest,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    let claims = crate::commands::guard::require_auth(&access_token, &auth)?;
    if claims.scope != "global" {
        return Err("Only super admins can assign modules".into());
    }

    // Delete existing modules for this tenant
    sqlx::query("DELETE FROM tenant_modules WHERE tenant_id = ?")
        .bind(&request.tenant_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Failed to clear existing modules: {}", e))?;

    // Insert new modules
    for module_id in &request.module_ids {
        let tm_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO tenant_modules (id, tenant_id, module_id, is_active) VALUES (?, ?, ?, 1)")
            .bind(&tm_id)
            .bind(&request.tenant_id)
            .bind(module_id)
            .execute(&auth.pool)
            .await
            .map_err(|e| format!("Failed to assign module {}: {}", module_id, e))?;
    }

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::PermissionChange,
        Some(&format!("Updated modules for tenant {}: {} modules assigned", request.tenant_id, request.module_ids.len())),
    )
    .await;

    Ok(())
}

#[tauri::command]
pub async fn delete_tenant(
    access_token: String,
    tenant_id: String,
    auth: State<'_, Arc<AuthManager>>,
) -> Result<(), String> {
    let claims = crate::commands::guard::require_auth(&access_token, &auth)?;
    if claims.scope != "global" {
        return Err("Only super admins can delete tenants".into());
    }

    // Check if it's the only tenant or if it has users? 
    // The schema has ON DELETE CASCADE for users, departments, etc.

    sqlx::query("DELETE FROM tenants WHERE id = ?")
        .bind(&tenant_id)
        .execute(&auth.pool)
        .await
        .map_err(|e| format!("Failed to delete tenant: {}", e))?;

    let _ = audit::log(
        &auth.pool,
        Some(&claims.sub),
        AuditAction::UserDelete,
        Some(&format!("Deleted tenant: {}", tenant_id)),
    )
    .await;

    Ok(())
}
