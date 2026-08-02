//! Zone event reporter — forwards cross-room exit/entry events to the Frappe
//! `wave_care` app via HTTP.
//!
//! Credentials are picked up from the same env vars used by the desktop
//! frappe_client: WAVE_FRAPPE_URL, WAVE_FRAPPE_API_KEY,
//! WAVE_FRAPPE_API_SECRET.  When unconfigured, all calls are silently skipped
//! so the sensing loop is never blocked.
//!
//! NOTE: ZoneReporter is constructed in main.rs but integration into the
//! cross_room event path (wiring record_exit/match_entry callbacks to call
//! report_zone_event) is a follow-up step.

use reqwest::Client;
use serde_json::json;

pub struct ZoneReporter {
    client: Client,
    frappe_url: String,
    /// Pre-formatted "token key:secret" header value, or None when unconfigured.
    frappe_auth: Option<String>,
}

impl ZoneReporter {
    pub fn new() -> Self {
        let frappe_url = std::env::var("WAVE_FRAPPE_URL")
            .unwrap_or_else(|_| "http://localhost:8080".to_string());
        let auth = {
            let key = std::env::var("WAVE_FRAPPE_API_KEY").unwrap_or_default();
            let secret = std::env::var("WAVE_FRAPPE_API_SECRET").unwrap_or_default();
            if !key.is_empty() && !secret.is_empty() {
                Some(format!("token {}:{}", key, secret))
            } else {
                None
            }
        };
        Self {
            client: Client::new(),
            frappe_url,
            frappe_auth: auth,
        }
    }

    /// Post a zone enter or exit event to Frappe.
    ///
    /// `event_type` should be "enter" or "exit".
    /// `vital_hr` / `vital_br` are optional instantaneous vitals at event time.
    /// Fire-and-forget — errors are logged but the caller is not blocked.
    pub async fn report_zone_event(
        &self,
        patient_token: &str,
        zone_id: &str,
        event_type: &str,
        vital_hr: Option<f32>,
        vital_br: Option<f32>,
    ) {
        let auth = match &self.frappe_auth {
            Some(a) => a.clone(),
            None => return,
        };
        let body = json!({
            "patient_token": patient_token,
            "zone_id": zone_id,
            "event_type": event_type,
            "vital_hr": vital_hr,
            "vital_br": vital_br,
        });
        let url = format!(
            "{}/api/method/wave_care.wave_care.api.zone_event",
            self.frappe_url
        );
        let result = self
            .client
            .post(&url)
            .header("Authorization", auth)
            .json(&body)
            .send()
            .await;
        if let Err(e) = result {
            tracing::warn!("zone_reporter: zone_event POST failed: {e}");
        }
    }

    /// Post a patient checkout event to Frappe (removes them from active list).
    pub async fn report_checkout(&self, patient_token: &str) {
        let auth = match &self.frappe_auth {
            Some(a) => a.clone(),
            None => return,
        };
        let body = json!({ "patient_token": patient_token });
        let url = format!(
            "{}/api/method/wave_care.wave_care.api.checkout_patient",
            self.frappe_url
        );
        let result = self
            .client
            .post(&url)
            .header("Authorization", auth)
            .json(&body)
            .send()
            .await;
        if let Err(e) = result {
            tracing::warn!("zone_reporter: checkout_patient POST failed: {e}");
        }
    }
}

impl Default for ZoneReporter {
    fn default() -> Self {
        Self::new()
    }
}
