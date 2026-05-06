//! First-launch database seeding.
//!
//! Seeds the 12 Wave-specific access modules on first launch.
//! No local Super Admin is seeded — the vendor's Super Admin
//! authenticates against the cloud License Server.

use sqlx::SqlitePool;

/// Wave-specific modules (code, name).
const WAVE_MODULES: &[(&str, &str, &str)] = &[
    ("mod-dashboard", "dashboard", "Dashboard"),
    ("mod-network", "network", "Network Management"),
    ("mod-firmware", "firmware", "Firmware Management"),
    ("mod-edge", "edge-modules", "Edge Modules"),
    ("mod-pi", "pi-nodes", "Pi Node Management"),
    ("mod-sensing", "sensing", "Sensing Server"),
    ("mod-mesh", "mesh", "Mesh Network"),
    ("mod-provision", "provisioning", "Device Provisioning"),
    ("mod-pose", "pose-3d", "3D Pose Visualization"),
    ("mod-settings", "settings", "System Settings"),
    ("mod-users", "user-management", "User Management"),
    ("mod-tenants", "tenant-management", "Tenant Management"),
];

/// Seed access modules if they don't exist.
pub async fn seed_modules(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let count: i32 =
        sqlx::query_scalar("SELECT COUNT(*) FROM access_modules")
            .fetch_one(pool)
            .await?;

    if count > 0 {
        tracing::info!("Access modules already seeded ({} modules)", count);
        return Ok(());
    }

    tracing::info!("Seeding {} access modules", WAVE_MODULES.len());

    for (id, code, name) in WAVE_MODULES {
        sqlx::query("INSERT OR IGNORE INTO access_modules (id, code, name) VALUES (?, ?, ?)")
            .bind(id)
            .bind(code)
            .bind(name)
            .execute(pool)
            .await?;
    }

    tracing::info!("Access modules seeded successfully");
    Ok(())
}

/// Create tenant_modules entries from a list of allowed module codes.
///
/// Called during license activation to configure which modules the
/// tenant has access to (based on their license tier).
pub async fn seed_tenant_modules(
    pool: &SqlitePool,
    tenant_id: &str,
    allowed_modules: &[String],
) -> Result<(), sqlx::Error> {
    for module_code in allowed_modules {
        // Look up the module ID by code
        let module_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM access_modules WHERE code = ?",
        )
        .bind(module_code)
        .fetch_optional(pool)
        .await?;

        if let Some(mid) = module_id {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO tenant_modules (id, tenant_id, module_id, is_active) \
                 VALUES (?, ?, ?, 1)",
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(&mid)
            .execute(pool)
            .await?;
        } else {
            tracing::warn!("Unknown module code in license: {}", module_code);
        }
    }

    Ok(())
}

/// Create a default "Admin" role with full permissions for the given tenant.
///
/// Called during license activation to give the first local admin full
/// access to all licensed modules.
pub async fn create_default_admin_role(
    pool: &SqlitePool,
    tenant_id: &str,
    allowed_modules: &[String],
) -> Result<String, sqlx::Error> {
    let role_id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO roles (id, name, description, tenant_id) \
         VALUES (?, 'Admin', 'Full access to all licensed modules', ?)",
    )
    .bind(&role_id)
    .bind(tenant_id)
    .execute(pool)
    .await?;

    // Create full permissions for each allowed module
    for module_code in allowed_modules {
        let module_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM access_modules WHERE code = ?",
        )
        .bind(module_code)
        .fetch_optional(pool)
        .await?;

        if let Some(mid) = module_id {
            // Create permission with full access
            let perm_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO permissions \
                 (id, can_read, can_add, can_edit, can_delete, can_approve, can_give_discount, \
                  read_scope, add_scope, edit_scope, delete_scope, approve_scope) \
                 VALUES (?, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1)",
            )
            .bind(&perm_id)
            .execute(pool)
            .await?;

            // Link role → module → permission
            let rm_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO role_modules (id, role_id, module_id, permission_id) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(&rm_id)
            .bind(&role_id)
            .bind(&mid)
            .bind(&perm_id)
            .execute(pool)
            .await?;
        }
    }

    tracing::info!("Created default Admin role for tenant {}", tenant_id);
    Ok(role_id)
}
