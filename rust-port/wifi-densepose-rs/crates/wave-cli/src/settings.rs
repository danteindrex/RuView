//! `wave-cli config get/set` — read/write the desktop app's settings.json
//! (`%APPDATA%\net.ruv.wave\settings.json`), the same file the Tauri settings
//! page persists. Missing file ⇒ an empty object (the app fills defaults).

use anyhow::{Context, Result};
use serde_json::Value;

fn settings_path() -> Result<std::path::PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(std::path::PathBuf::from)
    } else {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
    }
    .context("cannot resolve the app data directory")?;
    Ok(base.join("net.ruv.wave").join("settings.json"))
}

fn load() -> Result<Value> {
    let p = settings_path()?;
    match std::fs::read_to_string(&p) {
        Ok(s) => Ok(serde_json::from_str(&s).unwrap_or_else(|_| Value::Object(Default::default()))),
        Err(_) => Ok(Value::Object(Default::default())),
    }
}

/// Get all settings, or a single key.
pub fn get(key: Option<&str>) -> Result<Value> {
    let v = load()?;
    Ok(match key {
        Some(k) => v.get(k).cloned().unwrap_or(Value::Null),
        None => v,
    })
}

/// Set one key. Value is parsed as JSON when possible, else stored as a string.
pub fn set(key: &str, value: &str) -> Result<()> {
    let p = settings_path()?;
    let mut v = load()?;
    let obj = v.as_object_mut().context("settings.json is not an object")?;
    let parsed: Value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
    obj.insert(key.to_string(), parsed);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(&p, serde_json::to_string_pretty(&v)?)
        .with_context(|| format!("writing {}", p.display()))?;
    Ok(())
}
