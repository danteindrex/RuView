//! Thin REST client for the sensing-server API (backend "A").
//!
//! Base URL precedence (clig.dev config order): `--url` flag › `WAVE_URL` env ›
//! default `http://127.0.0.1:4000`. All calls have a timeout so the CLI never
//! hangs on a down server.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;

pub const DEFAULT_URL: &str = "http://127.0.0.1:4000";

pub struct Client {
    base: String,
    http: reqwest::Client,
}

impl Client {
    /// Resolve the base URL from flag/env/default and build a timed client.
    pub fn new(url_flag: Option<&str>) -> Result<Self> {
        let base = url_flag
            .map(|s| s.to_string())
            .or_else(|| std::env::var("WAVE_URL").ok())
            .unwrap_or_else(|| DEFAULT_URL.to_string());
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("building HTTP client")?;
        Ok(Client {
            base: base.trim_end_matches('/').to_string(),
            http,
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// GET `<base><path>` and deserialize JSON.
    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url} (is the sensing server running? try `wave-cli server start`)"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("GET {url} → HTTP {status}: {}", body.trim());
        }
        serde_json::from_str(&body).with_context(|| format!("parsing response from {url}"))
    }

    /// POST `<base><path>` (optionally with a JSON body) and deserialize JSON.
    pub async fn post_json<B: serde::Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: Option<&B>,
    ) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let mut req = self.http.post(&url);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.with_context(|| format!("POST {url}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("POST {url} → HTTP {status}: {}", text.trim());
        }
        if text.trim().is_empty() {
            // Some endpoints return 200 with no body.
            return serde_json::from_str("null").context("empty body");
        }
        serde_json::from_str(&text).with_context(|| format!("parsing response from {url}"))
    }

    /// DELETE `<base><path>`; returns the (possibly empty) JSON body as a value.
    pub async fn delete(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.base, path);
        let resp = self.http.delete(&url).send().await.with_context(|| format!("DELETE {url}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("DELETE {url} → HTTP {status}: {}", text.trim());
        }
        Ok(serde_json::from_str(&text).unwrap_or(serde_json::Value::Null))
    }

    /// True if the server answers `/health` within the timeout.
    pub async fn healthy(&self) -> bool {
        let url = format!("{}/health", self.base);
        matches!(self.http.get(&url).send().await, Ok(r) if r.status().is_success())
    }
}
