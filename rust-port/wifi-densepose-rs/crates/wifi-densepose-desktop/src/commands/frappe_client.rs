use reqwest::{Client, RequestBuilder};
use serde_json::Value;

fn frappe_url() -> String {
    std::env::var("WAVE_FRAPPE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string())
}

fn auth_header() -> Option<String> {
    let key = std::env::var("WAVE_FRAPPE_API_KEY").ok()?;
    let secret = std::env::var("WAVE_FRAPPE_API_SECRET").ok()?;
    if key.is_empty() || secret.is_empty() {
        return None;
    }
    Some(format!("token {}:{}", key, secret))
}

pub fn client() -> Client {
    Client::new()
}

pub fn post_method(method: &str) -> RequestBuilder {
    let url = format!("{}/api/method/{}", frappe_url(), method);
    let req = client().post(url);
    if let Some(auth) = auth_header() {
        req.header("Authorization", auth)
    } else {
        req
    }
}

pub fn get_method(method: &str) -> RequestBuilder {
    let url = format!("{}/api/method/{}", frappe_url(), method);
    let req = client().get(url);
    if let Some(auth) = auth_header() {
        req.header("Authorization", auth)
    } else {
        req
    }
}

pub fn get_resource(doctype: &str) -> RequestBuilder {
    let encoded = doctype.replace(' ', "%20");
    let url = format!("{}/api/resource/{}", frappe_url(), encoded);
    let req = client().get(url);
    if let Some(auth) = auth_header() {
        req.header("Authorization", auth)
    } else {
        req
    }
}

/// Unwrap `{"message": ...}` response from Frappe method calls.
pub async fn unwrap_message(resp: reqwest::Response) -> Result<Value, String> {
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if status.is_success() {
        Ok(body.get("message").cloned().unwrap_or(body))
    } else {
        let msg = body
            .get("exception")
            .or(body.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Frappe error")
            .to_string();
        Err(msg)
    }
}

/// Unwrap `{"data": [...]}` response from Frappe resource list calls.
pub async fn unwrap_data(resp: reqwest::Response) -> Result<Value, String> {
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    if status.is_success() {
        Ok(body.get("data").cloned().unwrap_or(body))
    } else {
        Err(format!("Frappe error: {}", status))
    }
}
