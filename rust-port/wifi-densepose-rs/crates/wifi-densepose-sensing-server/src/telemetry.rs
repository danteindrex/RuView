use std::time::Duration;
use opentelemetry_otlp::WithExportConfig;

/// Initialize Langfuse OTLP tracing. Returns None gracefully if env vars not set.
pub fn init_langfuse_tracing() -> Option<()> {
    let public_key = std::env::var("LANGFUSE_PUBLIC_KEY").ok()?;
    let secret_key = std::env::var("LANGFUSE_SECRET_KEY").ok()?;
    let host = std::env::var("LANGFUSE_HOST")
        .unwrap_or_else(|_| "https://cloud.langfuse.com".to_string());
    let endpoint = format!("{}/api/public/otel", host.trim_end_matches('/'));

    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let auth = B64.encode(format!("{}:{}", public_key, secret_key));

    let mut headers = std::collections::HashMap::new();
    headers.insert("Authorization".to_string(), format!("Basic {}", auth));
    headers.insert("x-langfuse-ingestion-version".to_string(), "4".to_string());

    let exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_endpoint(&endpoint)
        .with_headers(headers)
        .with_timeout(Duration::from_secs(10));

    let result = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            opentelemetry_sdk::trace::config()
                .with_sampler(opentelemetry_sdk::trace::Sampler::AlwaysOn)
                .with_resource(opentelemetry_sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", "wifi-densepose-sensing-server"),
                ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio);

    match result {
        Ok(_provider) => {
            tracing::info!("Langfuse OTLP tracing active → {}", endpoint);
            Some(())
        }
        Err(e) => {
            tracing::warn!("Langfuse init failed: {}", e);
            None
        }
    }
}

pub fn shutdown_tracer() {
    opentelemetry::global::shutdown_tracer_provider();
}
