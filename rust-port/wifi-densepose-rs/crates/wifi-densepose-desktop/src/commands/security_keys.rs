use keyring::Entry;

#[tauri::command]
pub async fn set_ssh_key(key_pem: String, passphrase: String) -> Result<(), String> {
    if !key_pem.trim_start().starts_with("-----BEGIN") {
        return Err("Invalid PEM: must start with -----BEGIN".to_string());
    }
    Entry::new("ruview-desktop", "ssh-key")
        .map_err(|e| e.to_string())?
        .set_password(&key_pem)
        .map_err(|e| e.to_string())?;
    if !passphrase.is_empty() {
        Entry::new("ruview-desktop", "ssh-passphrase")
            .map_err(|e| e.to_string())?
            .set_password(&passphrase)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn has_ssh_key() -> bool {
    Entry::new("ruview-desktop", "ssh-key")
        .ok()
        .and_then(|e| e.get_password().ok())
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}
