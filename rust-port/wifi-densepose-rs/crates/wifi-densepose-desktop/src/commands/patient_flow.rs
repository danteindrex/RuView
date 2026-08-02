//! Patient Flow Intelligence — Tauri commands.
//!
//! Wraps the Frappe wave_care patient-flow API and, on arrival, triggers a
//! CSI enrollment window on the sensing server so the patient's AETHER
//! re-ID embedding is captured and stored.

use serde::{Deserialize, Serialize};
use super::frappe_client as fc;

// ── Arrival / enroll ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct EnrollmentResult {
    pub patient_token: String,
    pub visit_name: String,
    pub check_in_time: String,
    pub enrollment_status: String,
}

/// Register patient arrival with Frappe and trigger a CSI enrollment window on
/// the sensing server.
///
/// Steps:
/// 1. POST to Frappe → receive `patient_token` and `visit_name`.
/// 2. Spawn a background task that POSTs `/enroll` to the sensing server and,
///    when the window completes, stores the averaged embedding back in Frappe.
///
/// The Tauri command returns immediately after step 1; the enrollment window
/// runs asynchronously so the UI is not blocked for the full duration.
#[tauri::command]
pub async fn register_patient_arrival(
    patient_id: String,
    node_id: String,
    sensing_server_url: String,
    enrollment_duration_seconds: Option<u64>,
) -> Result<EnrollmentResult, String> {
    // 1. Register with Frappe.
    let body = serde_json::json!({
        "patient_id": patient_id,
        "node_id": node_id,
    });
    let resp = fc::post_method("wave_care.wave_care.api.register_patient_arrival")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let frappe_result = fc::unwrap_message(resp).await?;

    let patient_token = frappe_result
        .get("patient_token")
        .and_then(|v| v.as_str())
        .ok_or("missing patient_token from Frappe")?
        .to_string();
    let visit_name = frappe_result
        .get("visit_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let check_in_time = frappe_result
        .get("check_in_time")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 2. Launch enrollment window in background.
    let duration = enrollment_duration_seconds.unwrap_or(10);
    let enroll_url = format!("{}/enroll", sensing_server_url.trim_end_matches('/'));
    let enroll_body = serde_json::json!({
        "patient_token": patient_token,
        "duration_seconds": duration,
    });
    let token_clone = patient_token.clone();
    let frappe_url = std::env::var("WAVE_FRAPPE_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    let api_key = std::env::var("WAVE_FRAPPE_API_KEY").unwrap_or_default();
    let api_secret = std::env::var("WAVE_FRAPPE_API_SECRET").unwrap_or_default();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let send_result = client
            .post(&enroll_url)
            .json(&enroll_body)
            .timeout(std::time::Duration::from_secs(duration + 5))
            .send()
            .await;

        let embedding_json = match send_result {
            Ok(r) => match r.json::<serde_json::Value>().await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("patient_flow: enrollment response parse error: {e}");
                    return;
                }
            },
            Err(e) => {
                tracing::warn!("patient_flow: enrollment request failed: {e}");
                return;
            }
        };

        let confidence = embedding_json
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let embedding_str = embedding_json
            .get("embedding")
            .map(|v| v.to_string())
            .unwrap_or_default();

        // Store the averaged embedding in Frappe.
        if api_key.is_empty() {
            return; // Not configured — skip silently.
        }
        let store_body = serde_json::json!({
            "patient_token": token_clone,
            "embedding_vector": embedding_str,
            "confidence_score": confidence,
        });
        let _ = client
            .post(format!(
                "{}/api/method/wave_care.wave_care.api.store_enrollment",
                frappe_url
            ))
            .header("Authorization", format!("token {}:{}", api_key, api_secret))
            .json(&store_body)
            .send()
            .await;
    });

    Ok(EnrollmentResult {
        patient_token,
        visit_name,
        check_in_time,
        enrollment_status: format!("Enrolling ({}s window started)", duration),
    })
}

// ── Active patients ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_active_patients() -> Result<serde_json::Value, String> {
    let resp = fc::get_method("wave_care.wave_care.api.get_active_patients")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}

// ── Checkout ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn checkout_patient_visit(patient_token: String) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({ "patient_token": patient_token });
    let resp = fc::post_method("wave_care.wave_care.api.checkout_patient")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}

// ── Analytics ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_zone_analytics(
    zone_id: String,
    days: Option<i64>,
) -> Result<serde_json::Value, String> {
    let d = days.unwrap_or(7).to_string();
    let resp = fc::get_method("wave_care.wave_care.api.get_zone_analytics")
        .query(&[("zone_id", zone_id.as_str()), ("days", d.as_str())])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}

#[tauri::command]
pub async fn get_patient_journey(
    visit_date: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut req = fc::get_method("wave_care.wave_care.api.get_patient_journey");
    if let Some(date) = visit_date {
        req = req.query(&[("visit_date", date)]);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}

// ── Queue simulation ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn simulate_queue_capacity(
    zone_id: String,
    current_servers: i64,
    arrival_rate_per_hour: f64,
    mean_service_minutes: f64,
) -> Result<serde_json::Value, String> {
    let servers = current_servers.to_string();
    let arrival = arrival_rate_per_hour.to_string();
    let service = mean_service_minutes.to_string();
    let resp = fc::get_method("wave_care.wave_care.api.simulate_queue_capacity")
        .query(&[
            ("zone_id", zone_id.as_str()),
            ("current_servers", servers.as_str()),
            ("arrival_rate_per_hour", arrival.as_str()),
            ("mean_service_minutes", service.as_str()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    fc::unwrap_message(resp).await
}
