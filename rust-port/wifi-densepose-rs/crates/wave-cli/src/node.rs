//! Node-direct HTTP (backend "C"): OTA firmware updates and WASM module
//! management. These hit each node's own HTTP servers (OTA :8032, WASM :8033),
//! not the sensing-server. Mirrors the desktop's contracts exactly, including
//! HMAC-SHA256 PSK signing for OTA.

use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use reqwest::multipart::{Form, Part};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

const OTA_PORT: u16 = 8032;
const WASM_PORT: u16 = 8033;
type HmacSha256 = Hmac<Sha256>;

fn client(secs: u64) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(secs))
        .build()
        .context("building HTTP client")
}

/// OTA-flash a node: multipart firmware + SHA-256 + optional HMAC-SHA256(PSK) sig.
pub async fn ota_update(node_ip: &str, file: &str, psk: Option<&str>) -> Result<Value> {
    let data = std::fs::read(file).with_context(|| format!("reading firmware {file}"))?;
    let hash = hex::encode(Sha256::digest(&data));
    let sig = match psk {
        Some(k) => {
            let mut mac = HmacSha256::new_from_slice(k.as_bytes())
                .map_err(|e| anyhow::anyhow!("invalid PSK: {e}"))?;
            mac.update(&data);
            Some(hex::encode(mac.finalize().into_bytes()))
        }
        None => None,
    };
    let part = Part::bytes(data.clone())
        .file_name("firmware.bin")
        .mime_str("application/octet-stream")?;
    let form = Form::new()
        .part("firmware", part)
        .text("sha256", hash.clone())
        .text("size", data.len().to_string());
    let url = format!("http://{node_ip}:{OTA_PORT}/ota/upload");
    let mut req = client(120)?.post(&url).multipart(form).header("X-OTA-SHA256", &hash);
    if let Some(s) = &sig {
        req = req.header("X-OTA-Signature", s);
    }
    let resp = req.send().await.with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("OTA {url} → HTTP {status}: {}", text.trim());
    }
    Ok(serde_json::from_str(&text).unwrap_or(json!({ "ok": true, "node": node_ip })))
}

pub async fn ota_check(node_ip: &str) -> Result<Value> {
    let url = format!("http://{node_ip}:{OTA_PORT}/ota/status");
    match client(5)?.get(&url).send().await {
        Ok(r) => {
            let reachable = r.status().is_success();
            let text = r.text().await.unwrap_or_default();
            Ok(serde_json::from_str(&text).unwrap_or(json!({ "reachable": reachable })))
        }
        Err(e) => Ok(json!({ "reachable": false, "error": e.to_string() })),
    }
}

pub async fn wasm_list(node_ip: &str) -> Result<Value> {
    let url = format!("http://{node_ip}:{WASM_PORT}/wasm/list");
    let resp = client(10)?.get(&url).send().await.with_context(|| format!("GET {url}"))?;
    let text = resp.text().await.unwrap_or_default();
    Ok(serde_json::from_str(&text).unwrap_or(json!({ "response": text })))
}

pub async fn wasm_upload(node_ip: &str, file: &str, name: &str, auto_start: bool) -> Result<Value> {
    let data = std::fs::read(file).with_context(|| format!("reading module {file}"))?;
    let hash = hex::encode(Sha256::digest(&data));
    let part = Part::bytes(data.clone())
        .file_name(format!("{name}.wasm"))
        .mime_str("application/wasm")?;
    let form = Form::new()
        .part("wasm", part)
        .text("name", name.to_string())
        .text("sha256", hash)
        .text("size", data.len().to_string())
        .text("auto_start", auto_start.to_string());
    let url = format!("http://{node_ip}:{WASM_PORT}/wasm/upload");
    let resp = client(60)?.post(&url).multipart(form).send().await.with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("WASM upload {url} → HTTP {status}: {}", text.trim());
    }
    Ok(serde_json::from_str(&text).unwrap_or(json!({ "ok": true })))
}

pub async fn wasm_info(node_ip: &str, id: &str) -> Result<Value> {
    let url = format!("http://{node_ip}:{WASM_PORT}/wasm/{id}");
    let text = client(10)?.get(&url).send().await.with_context(|| format!("GET {url}"))?.text().await.unwrap_or_default();
    Ok(serde_json::from_str(&text).unwrap_or(json!({ "response": text })))
}

pub async fn wasm_stats(node_ip: &str) -> Result<Value> {
    let url = format!("http://{node_ip}:{WASM_PORT}/wasm/stats");
    let text = client(10)?.get(&url).send().await.with_context(|| format!("GET {url}"))?.text().await.unwrap_or_default();
    Ok(serde_json::from_str(&text).unwrap_or(json!({ "response": text })))
}

pub async fn wasm_support(node_ip: &str) -> Result<Value> {
    let url = format!("http://{node_ip}:{WASM_PORT}/wasm/support");
    match client(5)?.get(&url).send().await {
        Ok(r) => {
            let ok = r.status().is_success();
            let text = r.text().await.unwrap_or_default();
            Ok(serde_json::from_str(&text).unwrap_or(json!({ "supported": ok })))
        }
        Err(e) => Ok(json!({ "supported": false, "error": e.to_string() })),
    }
}

pub async fn wasm_control(node_ip: &str, id: &str, action: &str) -> Result<Value> {
    let url = format!("http://{node_ip}:{WASM_PORT}/wasm/{id}/{action}");
    let resp = client(15)?.post(&url).send().await.with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("WASM {action} {url} → HTTP {status}: {}", text.trim());
    }
    Ok(serde_json::from_str(&text).unwrap_or(json!({ "ok": true })))
}
