//! Node-to-zone map — resolves a sensing node identifier to a Frappe `Zone`
//! zone_id at runtime.
//!
//! Each sensing node (ESP32 or Raspberry Pi) is associated with a clinic room
//! via the Frappe `Zone` DocType, which has a `deployment_id` field.  Operators
//! set `deployment_id` to the node's identifier string (e.g. "1", "10", or the
//! node's IP address).
//!
//! The map is fetched from Frappe at startup and refreshed every N seconds in
//! the background.  When Frappe is not configured (no API key), the map stays
//! empty and zone lookup falls back to the raw node identifier, so the sensing
//! loop is never blocked.
//!
//! Env vars (checked in order; same as zone_reporter.rs):
//!   WAVE_FRAPPE_URL       — base URL (default http://localhost:8080)
//!   WAVE_FRAPPE_API_KEY   — API key
//!   WAVE_FRAPPE_API_SECRET — API secret
//! Aliases (also accepted):
//!   RUVIEW_FRAPPE_URL, RUVIEW_FRAPPE_API_KEY, RUVIEW_FRAPPE_API_SECRET

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::Deserialize;

/// Maps sensing node identifiers (node_id string) to zone_id strings.
/// Populated from Frappe `Zone` DocType at startup and refreshed periodically.
pub type NodeZoneMap = Arc<RwLock<HashMap<String, String>>>;

#[derive(Debug, Deserialize)]
struct FrappeZone {
    zone_id: Option<String>,
    deployment_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FrappeListResponse {
    data: Vec<FrappeZone>,
}

/// Fetch zone→deployment mapping from Frappe and build node→zone lookup.
/// Returns an empty map if Frappe is not configured or the request fails.
pub async fn fetch_node_zone_map() -> HashMap<String, String> {
    let frappe_url = std::env::var("WAVE_FRAPPE_URL")
        .or_else(|_| std::env::var("RUVIEW_FRAPPE_URL"))
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    let api_key = std::env::var("WAVE_FRAPPE_API_KEY")
        .or_else(|_| std::env::var("RUVIEW_FRAPPE_API_KEY"))
        .unwrap_or_default();
    let api_secret = std::env::var("WAVE_FRAPPE_API_SECRET")
        .or_else(|_| std::env::var("RUVIEW_FRAPPE_API_SECRET"))
        .unwrap_or_default();

    if api_key.is_empty() {
        return HashMap::new();
    }

    let client = reqwest::Client::new();
    // Fetch zone_id and deployment_id from all Zone records.
    // deployment_id is set by the operator to match the node identifier string
    // (e.g. "1" for ESP32 node_id 1, "10" for Pi node_id 10).
    let url = format!(
        "{}/api/resource/Zone?fields=[%22zone_id%22,%22deployment_id%22]&limit=500",
        frappe_url
    );
    let result = client
        .get(&url)
        .header("Authorization", format!("token {}:{}", api_key, api_secret))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    let mut map = HashMap::new();
    match result {
        Ok(resp) => {
            match resp.json::<FrappeListResponse>().await {
                Ok(body) => {
                    for zone in body.data {
                        if let (Some(zone_id), Some(deployment_id)) =
                            (zone.zone_id, zone.deployment_id)
                        {
                            if !zone_id.is_empty() && !deployment_id.is_empty() {
                                map.insert(deployment_id, zone_id);
                            }
                        }
                    }
                    tracing::debug!("node_zone_map: loaded {} zone mappings", map.len());
                }
                Err(e) => {
                    tracing::warn!("node_zone_map: failed to parse Frappe Zone response: {e}");
                }
            }
        }
        Err(e) => {
            tracing::warn!("node_zone_map: Frappe Zone fetch failed: {e}");
        }
    }
    map
}

/// Create a shared `NodeZoneMap`, populate it immediately, and spawn a
/// background task that refreshes it every `refresh_interval_secs` seconds.
pub fn create_and_start(refresh_interval_secs: u64) -> NodeZoneMap {
    let map: NodeZoneMap = Arc::new(RwLock::new(HashMap::new()));
    let map_clone = map.clone();
    tokio::spawn(async move {
        loop {
            let fresh = fetch_node_zone_map().await;
            {
                let mut w = map_clone.write().await;
                *w = fresh;
            }
            tokio::time::sleep(std::time::Duration::from_secs(refresh_interval_secs)).await;
        }
    });
    map
}

/// Look up the zone_id for a node identifier.
///
/// The `node_key` should be the string form of the node's numeric id (e.g. "1",
/// "10").  Falls back to the raw `node_key` when no match is found, so
/// `PersonDetection.zone` always contains a meaningful value even before Frappe
/// is configured.
///
/// Uses `try_read()` so it never blocks the sensing loop — the only writer is
/// the background refresh task, which runs rarely (every 5 minutes).
pub fn zone_for_node_sync(map: &NodeZoneMap, node_key: &str) -> String {
    // Zone is looked up by deployment_id (set to node_id string or node IP in Frappe Zone config)
    match map.try_read() {
        Ok(r) => r.get(node_key).cloned().unwrap_or_else(|| node_key.to_string()),
        Err(_) => node_key.to_string(),
    }
}
