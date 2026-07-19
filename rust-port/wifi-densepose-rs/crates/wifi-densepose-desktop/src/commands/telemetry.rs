use serde::Deserialize;

#[derive(Deserialize)]
pub struct LangfuseConfig {
    pub enabled: bool,
    pub host: String,
    pub public_key: String,
    pub secret_key: String,
}

#[tauri::command]
pub async fn set_langfuse_config(config: LangfuseConfig) -> Result<(), String> {
    if config.enabled {
        std::env::set_var("LANGFUSE_HOST", &config.host);
        std::env::set_var("LANGFUSE_PUBLIC_KEY", &config.public_key);
        std::env::set_var("LANGFUSE_SECRET_KEY", &config.secret_key);
        tracing::info!("Langfuse tracing enabled → {}", config.host);
    } else {
        std::env::remove_var("LANGFUSE_PUBLIC_KEY");
        std::env::remove_var("LANGFUSE_SECRET_KEY");
        tracing::info!("Langfuse tracing disabled");
    }
    // TODO: persist to settings table via AppState
    Ok(())
}

#[tauri::command]
pub async fn get_langfuse_config() -> Result<serde_json::Value, String> {
    let has_key = std::env::var("LANGFUSE_PUBLIC_KEY").is_ok();
    Ok(serde_json::json!({
        "enabled": has_key,
        "host": std::env::var("LANGFUSE_HOST").unwrap_or_else(|_| "https://cloud.langfuse.com".to_string()),
        "public_key_hint": std::env::var("LANGFUSE_PUBLIC_KEY").ok().map(|k| format!("{}...", &k[..4.min(k.len())])).unwrap_or_default(),
    }))
}
