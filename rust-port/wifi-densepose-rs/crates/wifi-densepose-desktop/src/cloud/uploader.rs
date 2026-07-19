use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub struct InsightUploader {
    client: reqwest::Client,
    pub endpoint: String,
    signing_key: Vec<u8>,
}

impl InsightUploader {
    pub fn new(endpoint: String, signing_key: Vec<u8>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            endpoint,
            signing_key,
        }
    }

    pub async fn upload_session(
        &self,
        session_id: &str,
        session_json: &[u8],
    ) -> Result<String, String> {
        // TODO: replace with AES-256-GCM encryption from DataEncryptor (feature/encryption-at-rest)
        let payload_b64 = B64.encode(session_json);
        let mut mac = HmacSha256::new_from_slice(&self.signing_key)
            .map_err(|e| e.to_string())?;
        mac.update(payload_b64.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let body = serde_json::json!({
            "session_id": session_id,
            "payload": payload_b64,
            "signature": signature,
        });
        let resp = self.client
            .post(format!("{}/ingest", self.endpoint))
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("server {}", resp.status()));
        }
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        Ok(json["session_id"].as_str().unwrap_or(session_id).to_string())
    }
}
