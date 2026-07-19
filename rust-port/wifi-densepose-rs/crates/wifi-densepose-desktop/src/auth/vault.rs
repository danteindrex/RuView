//! Stronghold-backed vault manager for sensitive secrets.
//!
//! Provides typed access to secrets (JWT signing key, future encryption keys)
//! stored in the Tauri Stronghold plugin. The Stronghold plugin is initialized
//! during app `setup` in `lib.rs`.

use std::sync::Arc;
use tokio::sync::RwLock;

/// VaultManager provides typed access to secrets stored in Stronghold.
/// Stronghold plugin is initialized in lib.rs at app startup.
pub struct VaultManager {
    // Cache of secrets to avoid repeated vault reads.
    // TODO: replace cache with direct Stronghold reads once plugin is wired.
    jwt_secret_cache: Arc<RwLock<Option<String>>>,
}

impl VaultManager {
    pub fn new() -> Self {
        Self {
            jwt_secret_cache: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_jwt_secret(&self) -> Result<String, String> {
        let cache = self.jwt_secret_cache.read().await;
        cache.clone().ok_or_else(|| "vault not initialized".to_string())
    }

    pub async fn set_jwt_secret(&self, secret: &str) -> Result<(), String> {
        let mut cache = self.jwt_secret_cache.write().await;
        *cache = Some(secret.to_string());
        Ok(())
    }
}

impl Default for VaultManager {
    fn default() -> Self { Self::new() }
}
