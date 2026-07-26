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

async fn open() -> Result<SqlitePool> {
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
