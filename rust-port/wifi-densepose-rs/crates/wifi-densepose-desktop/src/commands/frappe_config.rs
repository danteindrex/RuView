use keyring::Entry;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "ruview-frappe";
const KEY_URL: &str = "frappe-url";
const KEY_API_KEY: &str = "frappe-api-key";
const KEY_API_SECRET: &str = "frappe-api-secret";

#[derive(Debug, Serialize, Deserialize)]
pub struct FrappeConfigHint {
    pub url: String,
    pub api_key_hint: String,
    pub configured: bool,
}

#[tauri::command]
pub async fn get_frappe_config() -> Result<FrappeConfigHint, String> {
    let url = Entry::new(SERVICE, KEY_URL)
        .and_then(|e| e.get_password())
        .unwrap_or_else(|_| std::env::var("RUVIEW_FRAPPE_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string()));
    let api_key = Entry::new(SERVICE, KEY_API_KEY)
        .and_then(|e| e.get_password())
        .unwrap_or_else(|_| std::env::var("RUVIEW_FRAPPE_API_KEY").unwrap_or_default());
    let configured = !api_key.is_empty();
    let api_key_hint = if api_key.len() > 6 {
        format!("{}…", &api_key[..6])
    } else if !api_key.is_empty() {
        "set".to_string()
    } else {
        String::new()
    };
    Ok(FrappeConfigHint { url, api_key_hint, configured })
}

#[tauri::command]
pub async fn set_frappe_config(url: String, api_key: String, api_secret: String) -> Result<(), String> {
    // Persist to OS keychain
    Entry::new(SERVICE, KEY_URL)
        .map_err(|e| e.to_string())?
        .set_password(&url)
        .map_err(|e| e.to_string())?;
    if !api_key.is_empty() {
        Entry::new(SERVICE, KEY_API_KEY)
            .map_err(|e| e.to_string())?
            .set_password(&api_key)
            .map_err(|e| e.to_string())?;
    }
    if !api_secret.is_empty() {
        Entry::new(SERVICE, KEY_API_SECRET)
            .map_err(|e| e.to_string())?
            .set_password(&api_secret)
            .map_err(|e| e.to_string())?;
    }
    // Also set env vars so frappe_client.rs picks them up immediately
    // without restarting the app.
    std::env::set_var("RUVIEW_FRAPPE_URL", &url);
    if !api_key.is_empty() {
        std::env::set_var("RUVIEW_FRAPPE_API_KEY", &api_key);
    }
    if !api_secret.is_empty() {
        std::env::set_var("RUVIEW_FRAPPE_API_SECRET", &api_secret);
    }
    Ok(())
}

/// Called at app startup to load persisted Frappe credentials into env vars
/// so frappe_client.rs works without the user having to re-enter them.
pub fn load_frappe_config_into_env() {
    if let Ok(entry) = Entry::new(SERVICE, KEY_URL) {
        if let Ok(url) = entry.get_password() {
            if !url.is_empty() {
                std::env::set_var("RUVIEW_FRAPPE_URL", url);
            }
        }
    }
    if let Ok(entry) = Entry::new(SERVICE, KEY_API_KEY) {
        if let Ok(key) = entry.get_password() {
            if !key.is_empty() {
                std::env::set_var("RUVIEW_FRAPPE_API_KEY", key);
            }
        }
    }
    if let Ok(entry) = Entry::new(SERVICE, KEY_API_SECRET) {
        if let Ok(secret) = entry.get_password() {
            if !secret.is_empty() {
                std::env::set_var("RUVIEW_FRAPPE_API_SECRET", secret);
            }
        }
    }
}
