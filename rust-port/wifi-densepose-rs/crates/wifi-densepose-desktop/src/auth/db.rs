//! SQLite database pool initialization and schema migrations.
//!
//! Creates a `wave.db` file in the Tauri app data directory.
//! Runs embedded SQL migrations on first launch and on schema upgrades.

// SQLCIPHER ENCRYPTION TODO:
// To encrypt wave.db at rest:
//   1. Add `features = ["sqlcipher"]` to sqlx in Cargo.toml
//      (requires libsqlcipher or bundled-sqlcipher feature + openssl-sys)
//   2. Derive encryption key: HKDF-SHA256(vault_master_key, info="sqlcipher-key") → 32 bytes
//   3. Connection URL: "sqlite:{path}?_pragma=key%3D{64-hex-char-key}"
//   4. Existing DB migration: use SQLCipher's sqlcipher_export() to re-encrypt in-place
// This is gated behind VaultManager being fully wired (feature/encryption-at-rest).

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::PathBuf;
use std::str::FromStr;

/// Schema version — bump when adding migrations.
const SCHEMA_VERSION: i32 = 2;

/// All DDL statements for the Wave auth schema (18 tables).
const SCHEMA_SQL: &str = r#"
-- ═══════════════════════════════════════════════
-- Schema meta
-- ═══════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);

-- ═══════════════════════════════════════════════
-- License (cached from cloud server)
-- ═══════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS licenses (
    id TEXT PRIMARY KEY,
    license_key TEXT NOT NULL UNIQUE,
    tenant_id TEXT NOT NULL,
    tenant_name TEXT NOT NULL,
    tier TEXT NOT NULL DEFAULT 'starter',
    hardware_fingerprint TEXT,
    allowed_modules TEXT NOT NULL,
    max_users INTEGER NOT NULL DEFAULT 5,
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    last_validated_at TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    cached_response TEXT
);

-- ═══════════════════════════════════════════════
-- Core entities (mirroring Taha)
-- ═══════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS tenants (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    domain TEXT UNIQUE,
    contact TEXT,
    email TEXT,
    location TEXT,
    industry TEXT,
    currency_code TEXT DEFAULT 'USD',
    type TEXT DEFAULT 'COMPANY' CHECK(type IN ('ADMIN','COMPANY')),
    verification_status TEXT DEFAULT 'verified',
    logo TEXT,
    description TEXT,
    registered_by TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS departments (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    phone TEXT,
    avatar_url TEXT,
    password_hash TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    must_change_password INTEGER NOT NULL DEFAULT 0,
    email_verified INTEGER NOT NULL DEFAULT 1,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    department_id TEXT REFERENCES departments(id),
    last_login TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS roles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ═══════════════════════════════════════════════
-- Permission matrix
-- ═══════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS access_modules (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    dirs TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS permissions (
    id TEXT PRIMARY KEY,
    can_read INTEGER NOT NULL DEFAULT 0,
    can_add INTEGER NOT NULL DEFAULT 0,
    can_edit INTEGER NOT NULL DEFAULT 0,
    can_delete INTEGER NOT NULL DEFAULT 0,
    can_approve INTEGER NOT NULL DEFAULT 0,
    can_give_discount INTEGER NOT NULL DEFAULT 0,
    read_scope INTEGER NOT NULL DEFAULT 0,
    add_scope INTEGER NOT NULL DEFAULT 0,
    edit_scope INTEGER NOT NULL DEFAULT 0,
    delete_scope INTEGER NOT NULL DEFAULT 0,
    approve_scope INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS role_modules (
    id TEXT PRIMARY KEY,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    module_id TEXT NOT NULL REFERENCES access_modules(id),
    permission_id TEXT NOT NULL REFERENCES permissions(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS user_roles (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS tenant_modules (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    module_id TEXT NOT NULL REFERENCES access_modules(id),
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS department_roles (
    id TEXT PRIMARY KEY,
    department_id TEXT NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE
);

-- ═══════════════════════════════════════════════
-- Security & session
-- ═══════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id TEXT PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_agent TEXT,
    ip_address TEXT,
    revoked INTEGER NOT NULL DEFAULT 0,
    revoked_at TEXT,
    replaced_by_token TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    details TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    key_hash TEXT NOT NULL UNIQUE,
    key_prefix TEXT NOT NULL,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    generated_by TEXT NOT NULL REFERENCES users(id),
    expires_at TEXT,
    last_used_at TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','revoked','expired')),
    revoked_by TEXT,
    revoked_at TEXT,
    revoke_reason TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS service_tokens (
    id TEXT PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    service_name TEXT NOT NULL,
    description TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    permissions TEXT,
    expires_at TEXT,
    last_used_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ═══════════════════════════════════════════════
-- Enterprise Tenant Settings
-- ═══════════════════════════════════════════════
CREATE TABLE IF NOT EXISTS tenant_settings (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    whapi_token TEXT,
    whapi_number TEXT,
    alert_target_number TEXT,
    security_armed INTEGER NOT NULL DEFAULT 0,
    crowd_threshold INTEGER NOT NULL DEFAULT 5,
    fall_alert_enabled INTEGER NOT NULL DEFAULT 1,
    vitals_alert_enabled INTEGER NOT NULL DEFAULT 1,
    hr_min REAL DEFAULT 40.0,
    hr_max REAL DEFAULT 140.0,
    br_min REAL DEFAULT 8.0,
    br_max REAL DEFAULT 30.0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// Initialize the SQLite database pool and run migrations.
///
/// The database file is stored at `<app_data_dir>/wave.db`.
pub async fn init_pool(app_data_dir: &PathBuf) -> Result<SqlitePool, sqlx::Error> {
    let db_path = app_data_dir.join("wave.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    tracing::info!("Initializing SQLite database at {}", db_path.display());

    let options = SqliteConnectOptions::from_str(&db_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // Run schema migration
    run_migrations(&pool).await?;

    Ok(pool)
}

/// Run schema creation (idempotent via IF NOT EXISTS).
async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Check current schema version
    let has_version_table: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_version'",
    )
    .fetch_one(pool)
    .await?;

    let current_version: i32 = if has_version_table {
        sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_version")
            .fetch_one(pool)
            .await?
    } else {
        0
    };

    if current_version >= SCHEMA_VERSION {
        tracing::info!("Database schema is up to date (v{})", current_version);
        return Ok(());
    }

    tracing::info!(
        "Migrating database schema from v{} to v{}",
        current_version,
        SCHEMA_VERSION
    );

    // Execute all DDL statements
    sqlx::raw_sql(SCHEMA_SQL).execute(pool).await?;

    // Record schema version
    if current_version == 0 {
        sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
            .bind(SCHEMA_VERSION)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("UPDATE schema_version SET version = ?")
            .bind(SCHEMA_VERSION)
            .execute(pool)
            .await?;
    }

    tracing::info!("Database migration complete (v{})", SCHEMA_VERSION);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_init_pool_creates_db() {
        let tmp = TempDir::new().unwrap();
        let pool = init_pool(&tmp.path().to_path_buf()).await.unwrap();

        // Verify tables exist
        let count: i32 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='users'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);

        pool.close().await;
    }

    #[tokio::test]
    async fn test_migration_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_path_buf();

        // Run twice — should not error
        let pool1 = init_pool(&path).await.unwrap();
        pool1.close().await;

        let pool2 = init_pool(&path).await.unwrap();
        let version: i32 =
            sqlx::query_scalar("SELECT MAX(version) FROM schema_version")
                .fetch_one(&pool2)
                .await
                .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        pool2.close().await;
    }
}
