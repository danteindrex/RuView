//! Admin (backend "D") — read/write the desktop's `wave.db` (SQLite) directly
//! via sqlx (the version the workspace already links). Users/roles/tenants/
//! license/plan. Password hashing matches the desktop (Argon2id) so users
//! created here log in through the app.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

/// Locate `wave.db`: `%APPDATA%\net.ruv.wave\wave.db` (Windows) or the XDG dir.
fn db_path() -> Result<std::path::PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(std::path::PathBuf::from)
    } else {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
    }
    .context("cannot resolve the app data directory")?;
    let p = base.join("net.ruv.wave").join("wave.db");
    if !p.exists() {
        anyhow::bail!(
            "wave.db not found at {} — launch the desktop app once to initialize it",
            p.display()
        );
    }
    Ok(p)
}

pub(crate) async fn open() -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::new().filename(db_path()?);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .context("opening wave.db")
}

pub async fn user_list(filter_email: Option<&str>) -> Result<Value> {
    let pool = open().await?;
    let rows = sqlx::query(
        "SELECT email, first_name, last_name, is_active, tenant_id, COALESCE(last_login,'') AS last_login \
         FROM users ORDER BY email",
    )
    .fetch_all(&pool)
    .await?;
    let mut users: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "email": r.get::<String, _>("email"),
                "name": format!("{} {}", r.get::<String, _>("first_name"), r.get::<String, _>("last_name")),
                "active": r.get::<i64, _>("is_active") == 1,
                "tenant_id": r.get::<String, _>("tenant_id"),
                "last_login": r.get::<String, _>("last_login"),
            })
        })
        .collect();
    if let Some(e) = filter_email {
        users.retain(|u| u.get("email").and_then(|v| v.as_str()) == Some(e));
    }
    Ok(json!({ "users": users }))
}

pub async fn role_list() -> Result<Value> {
    let pool = open().await?;
    let rows = sqlx::query("SELECT name, description, tenant_id FROM roles ORDER BY name")
        .fetch_all(&pool)
        .await?;
    let roles: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "name": r.get::<String, _>("name"),
                "description": r.get::<String, _>("description"),
                "tenant_id": r.get::<String, _>("tenant_id"),
            })
        })
        .collect();
    Ok(json!({ "roles": roles }))
}

pub async fn tenant_list() -> Result<Value> {
    let pool = open().await?;
    let rows = sqlx::query("SELECT id, name, COALESCE(industry,'') AS industry, type FROM tenants ORDER BY name")
        .fetch_all(&pool)
        .await?;
    let tenants: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<String, _>("id"),
                "name": r.get::<String, _>("name"),
                "industry": r.get::<String, _>("industry"),
                "type": r.get::<String, _>("type"),
            })
        })
        .collect();
    Ok(json!({ "tenants": tenants }))
}

pub async fn license_status() -> Result<Value> {
    let pool = open().await?;
    let row = sqlx::query(
        "SELECT tenant_name, tier, max_users, expires_at, is_active FROM licenses \
         ORDER BY last_validated_at DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await?;
    Ok(match row {
        Some(r) => json!({
            "tenant": r.get::<String, _>("tenant_name"),
            "tier": r.get::<String, _>("tier"),
            "max_users": r.get::<i64, _>("max_users"),
            "expires_at": r.get::<String, _>("expires_at"),
            "active": r.get::<i64, _>("is_active") == 1,
        }),
        None => json!({ "licensed": false }),
    })
}

/// The 12 Wave access modules (id, code, name) — mirrors the desktop's seed.
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

/// This machine's hardware fingerprint (SHA-256 of the machine UID) — identical
/// to the desktop's `get_hardware_fingerprint`, so a key bound here validates in
/// the app.
fn hardware_fingerprint() -> String {
    match machine_uid::get() {
        Ok(uid) => {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(uid.as_bytes()))
        }
        Err(_) => "unknown".to_string(),
    }
}

#[derive(serde::Deserialize)]
struct LicenseServerResponse {
    valid: bool,
    tenant: Option<LicenseTenant>,
    tier: Option<String>,
    modules: Option<Vec<String>>,
    max_users: Option<i32>,
    expires_at: Option<String>,
    error: Option<String>,
}

#[derive(serde::Deserialize)]
struct LicenseTenant {
    id: String,
    name: String,
    industry: Option<String>,
}

/// Activate a license and seed the first-launch tenant/admin — mirrors the
/// desktop `activate_license` command end to end (license row, tenant,
/// tenant_modules, default Admin role + permissions, first admin user).
///
/// `dev` fabricates the same enterprise dev-tenant the desktop uses in debug
/// builds (no server call); otherwise POSTs to the license server.
#[allow(clippy::too_many_arguments)]
pub async fn license_activate(
    license_key: &str,
    server_url: &str,
    dev: bool,
    admin_email: &str,
    admin_first: &str,
    admin_last: &str,
    admin_password: &str,
) -> Result<Value> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    use argon2::Argon2;
    use rand_core::OsRng;

    let fingerprint = hardware_fingerprint();
    let app_version = env!("CARGO_PKG_VERSION");

    // 1. Resolve the license (dev bypass or license server).
    let resp: LicenseServerResponse = if dev {
        LicenseServerResponse {
            valid: true,
            tenant: Some(LicenseTenant {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Development Tenant".to_string(),
                industry: Some("Software".to_string()),
            }),
            tier: Some("enterprise".to_string()),
            modules: Some(WAVE_MODULES.iter().map(|(_, code, _)| code.to_string()).collect()),
            max_users: Some(50),
            expires_at: chrono::Utc::now()
                .checked_add_months(chrono::Months::new(12))
                .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
            error: None,
        }
    } else {
        let url = format!("{}/api/v1/license/activate", server_url.trim_end_matches('/'));
        let http = reqwest::Client::new();
        let r = http
            .post(&url)
            .json(&json!({
                "license_key": license_key,
                "hardware_fingerprint": fingerprint,
                "app_version": app_version,
            }))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .with_context(|| format!("cannot reach license server {url}"))?;
        let status = r.status();
        let body: LicenseServerResponse = r.json().await.context("invalid license server response")?;
        if !status.is_success() || !body.valid {
            anyhow::bail!("{}", body.error.unwrap_or_else(|| "invalid license key".into()));
        }
        body
    };

    let tenant = resp.tenant.as_ref().context("license server returned no tenant")?;
    let modules = resp.modules.clone().unwrap_or_default();
    let tier = resp.tier.as_deref().unwrap_or("starter");
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let expires = resp.expires_at.clone().unwrap_or_else(|| now.clone());

    let pool = open().await?;

    // 2. Ensure the 12 access modules exist (idempotent).
    for (id, code, name) in WAVE_MODULES {
        sqlx::query("INSERT OR IGNORE INTO access_modules (id, code, name) VALUES (?1, ?2, ?3)")
            .bind(id).bind(code).bind(name)
            .execute(&pool).await?;
    }

    // 3. Cache the license row.
    let modules_json = serde_json::to_string(&modules).unwrap_or_default();
    sqlx::query(
        "INSERT INTO licenses \
         (id, license_key, tenant_id, tenant_name, tier, hardware_fingerprint, allowed_modules, \
          max_users, issued_at, expires_at, last_validated_at, is_active) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?9, 1)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(license_key).bind(&tenant.id).bind(&tenant.name).bind(tier)
    .bind(&fingerprint).bind(&modules_json).bind(resp.max_users.unwrap_or(5))
    .bind(&now).bind(&expires)
    .execute(&pool).await.context("caching license")?;

    // 4. Create the tenant.
    sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name, industry, type, verification_status) \
         VALUES (?1, ?2, ?3, 'COMPANY', 'verified')",
    )
    .bind(&tenant.id).bind(&tenant.name).bind(&tenant.industry)
    .execute(&pool).await.context("creating tenant")?;

    // 5. Seed tenant_modules + a default Admin role with full permissions.
    let role_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO roles (id, name, description, tenant_id) \
         VALUES (?1, 'Admin', 'Full access to all licensed modules', ?2)",
    )
    .bind(&role_id).bind(&tenant.id)
    .execute(&pool).await.context("creating Admin role")?;

    for code in &modules {
        let mid: Option<String> = sqlx::query_scalar("SELECT id FROM access_modules WHERE code = ?1")
            .bind(code).fetch_optional(&pool).await?;
        let Some(mid) = mid else { continue };
        sqlx::query("INSERT OR IGNORE INTO tenant_modules (id, tenant_id, module_id, is_active) VALUES (?1, ?2, ?3, 1)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&tenant.id).bind(&mid)
            .execute(&pool).await?;
        let perm_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO permissions \
             (id, can_read, can_add, can_edit, can_delete, can_approve, can_give_discount, \
              read_scope, add_scope, edit_scope, delete_scope, approve_scope) \
             VALUES (?1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1)",
        )
        .bind(&perm_id).execute(&pool).await?;
        sqlx::query("INSERT INTO role_modules (id, role_id, module_id, permission_id) VALUES (?1, ?2, ?3, ?4)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&role_id).bind(&mid).bind(&perm_id)
            .execute(&pool).await?;
    }

    // 6. Create the first admin user + assign the Admin role.
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(admin_password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))?
        .to_string();
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users \
         (id, first_name, last_name, email, password_hash, is_active, must_change_password, tenant_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, 1, 1, ?6)",
    )
    .bind(&user_id).bind(admin_first).bind(admin_last).bind(admin_email).bind(hash).bind(&tenant.id)
    .execute(&pool).await.context("creating admin user (already exists?)")?;
    sqlx::query("INSERT INTO user_roles (id, user_id, role_id) VALUES (?1, ?2, ?3)")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&user_id).bind(&role_id)
        .execute(&pool).await.context("assigning Admin role")?;

    Ok(json!({
        "activated": true,
        "tenant": tenant.name,
        "tenant_id": tenant.id,
        "tier": tier,
        "modules": modules,
        "admin_user": admin_email,
        "expires_at": expires,
        "dev_bypass": dev,
    }))
}

/// Plan tier, derived from the active license tier.
pub async fn plan() -> Result<Value> {
    let lic = license_status().await?;
    let tier = lic.get("tier").and_then(|v| v.as_str()).unwrap_or("local");
    Ok(json!({ "tier": tier }))
}

pub async fn user_create(
    email: &str,
    first: &str,
    last: &str,
    password: &str,
    tenant: Option<&str>,
) -> Result<()> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    use argon2::Argon2;
    use rand_core::OsRng;

    let pool = open().await?;
    let tenant_id = match tenant {
        Some(t) => t.to_string(),
        None => sqlx::query("SELECT id FROM tenants LIMIT 1")
            .fetch_optional(&pool)
            .await?
            .map(|r| r.get::<String, _>("id"))
            .context("no tenant in wave.db — activate a license / create a tenant first")?,
    };
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))?
        .to_string();
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, first_name, last_name, email, password_hash, tenant_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(id)
    .bind(first)
    .bind(last)
    .bind(email)
    .bind(hash)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .context("inserting user (already exists?)")?;
    Ok(())
}

pub async fn user_delete(email: &str) -> Result<u64> {
    let pool = open().await?;
    let res = sqlx::query("DELETE FROM users WHERE email = ?1")
        .bind(email)
        .execute(&pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn user_update(
    email: &str,
    first: Option<&str>,
    last: Option<&str>,
    active: Option<bool>,
) -> Result<u64> {
    let pool = open().await?;
    let res = sqlx::query(
        "UPDATE users SET first_name = COALESCE(?1, first_name), \
         last_name = COALESCE(?2, last_name), is_active = COALESCE(?3, is_active), \
         updated_at = datetime('now') WHERE email = ?4",
    )
    .bind(first)
    .bind(last)
    .bind(active.map(|b| b as i64))
    .bind(email)
    .execute(&pool)
    .await?;
    Ok(res.rows_affected())
}

pub async fn user_assign_role(email: &str, role_name: &str) -> Result<()> {
    let pool = open().await?;
    let uid: String = sqlx::query_scalar("SELECT id FROM users WHERE email = ?1")
        .bind(email)
        .fetch_optional(&pool)
        .await?
        .context("user not found")?;
    let rid: String = sqlx::query_scalar("SELECT id FROM roles WHERE name = ?1 LIMIT 1")
        .bind(role_name)
        .fetch_optional(&pool)
        .await?
        .context("role not found")?;
    sqlx::query("INSERT INTO user_roles (id, user_id, role_id) VALUES (?1, ?2, ?3)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uid)
        .bind(rid)
        .execute(&pool)
        .await
        .context("assigning role (already assigned?)")?;
    Ok(())
}

pub async fn set_password(email: &str, new_password: &str) -> Result<u64> {
    use argon2::password_hash::{PasswordHasher, SaltString};
    use argon2::Argon2;
    use rand_core::OsRng;

    let pool = open().await?;
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(new_password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))?
        .to_string();
    let res = sqlx::query(
        "UPDATE users SET password_hash = ?1, must_change_password = 0, updated_at = datetime('now') \
         WHERE email = ?2",
    )
    .bind(hash)
    .bind(email)
    .execute(&pool)
    .await?;
    Ok(res.rows_affected())
}

async fn first_tenant(pool: &SqlitePool) -> Result<String> {
    sqlx::query_scalar("SELECT id FROM tenants LIMIT 1")
        .fetch_optional(pool)
        .await?
        .context("no tenant in wave.db — activate a license / create a tenant first")
}

pub async fn role_create(name: &str, description: &str, tenant: Option<&str>) -> Result<()> {
    let pool = open().await?;
    let tenant_id = match tenant {
        Some(t) => t.to_string(),
        None => first_tenant(&pool).await?,
    };
    sqlx::query("INSERT INTO roles (id, name, description, tenant_id) VALUES (?1, ?2, ?3, ?4)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(name)
        .bind(description)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .context("creating role")?;
    Ok(())
}

pub async fn role_delete(name: &str) -> Result<u64> {
    let pool = open().await?;
    let res = sqlx::query("DELETE FROM roles WHERE name = ?1")
        .bind(name)
        .execute(&pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn list_modules() -> Result<Value> {
    let pool = open().await?;
    let rows = sqlx::query("SELECT code, name FROM access_modules ORDER BY code")
        .fetch_all(&pool)
        .await?;
    let mods: Vec<Value> = rows
        .iter()
        .map(|r| json!({ "code": r.get::<String, _>("code"), "name": r.get::<String, _>("name") }))
        .collect();
    Ok(json!({ "modules": mods }))
}

pub async fn tenant_create(name: &str, industry: Option<&str>, kind: &str) -> Result<()> {
    let pool = open().await?;
    sqlx::query("INSERT INTO tenants (id, name, industry, type) VALUES (?1, ?2, ?3, ?4)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(name)
        .bind(industry)
        .bind(kind)
        .execute(&pool)
        .await
        .context("creating tenant")?;
    Ok(())
}

pub async fn tenant_delete(id_or_name: &str) -> Result<u64> {
    let pool = open().await?;
    let res = sqlx::query("DELETE FROM tenants WHERE id = ?1 OR name = ?1")
        .bind(id_or_name)
        .execute(&pool)
        .await?;
    Ok(res.rows_affected())
}

pub async fn tenant_assign_modules(tenant_id: &str, module_codes: &[String]) -> Result<u64> {
    let pool = open().await?;
    let mut n = 0u64;
    for code in module_codes {
        let mid: Option<String> = sqlx::query_scalar("SELECT id FROM access_modules WHERE code = ?1")
            .bind(code)
            .fetch_optional(&pool)
            .await?;
        if let Some(module_id) = mid {
            let res = sqlx::query(
                "INSERT INTO tenant_modules (id, tenant_id, module_id, is_active) VALUES (?1, ?2, ?3, 1)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(tenant_id)
            .bind(module_id)
            .execute(&pool)
            .await;
            if res.is_ok() {
                n += 1;
            }
        }
    }
    Ok(n)
}
