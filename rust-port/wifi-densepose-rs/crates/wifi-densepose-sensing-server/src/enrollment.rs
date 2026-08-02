//! Patient CSI enrollment endpoint.
//!
//! POST /enroll — opens a time-bounded embedding capture window for a patient
//! token, averages the embeddings collected by the sensing loop, and returns
//! the result.
//!
//! The sensing loop (in main.rs) calls [`push_enrollment_frame`] each time a
//! pose embedding is produced; if that token is enrolled the frame is buffered
//! here.  When the window expires the averaged embedding is returned.
//!
//! NOTE: push_enrollment_frame integration into the sensing tick loop is a
//! follow-up step; until it is wired, /enroll returns a zero-embedding with
//! samples_captured=0.  AETHER re-ID embeddings are 128-dim; the zero-vector
//! fallback uses the same dimension.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

/// Shared enrollment state — stores the in-progress embedding buffer per token.
pub type EnrollmentStore = Arc<Mutex<std::collections::HashMap<String, Vec<Vec<f32>>>>>;

#[derive(Debug, Deserialize)]
pub struct EnrollRequest {
    pub patient_token: String,
    /// Duration of the capture window (clamped to 30 s max).
    pub duration_seconds: u64,
}

#[derive(Debug, Serialize)]
pub struct EnrollResponse {
    pub patient_token: String,
    pub embedding: Vec<f32>,
    pub dimension: usize,
    pub confidence: f32,
    pub samples_captured: usize,
}

/// AETHER re-ID embedding dimension used throughout the codebase.
const EMBEDDING_DIM: usize = 128;

pub async fn handle_enroll(
    State(store): State<EnrollmentStore>,
    Json(req): Json<EnrollRequest>,
) -> Json<EnrollResponse> {
    let token = req.patient_token.clone();
    let duration = req.duration_seconds.min(30);

    // Open an empty buffer for this token; any previous buffer is discarded.
    {
        let mut map = store.lock().await;
        map.insert(token.clone(), Vec::new());
    }

    // Wait for the sensing loop to push frames via push_enrollment_frame().
    sleep(Duration::from_secs(duration)).await;

    // Drain the buffer.
    let frames = {
        let mut map = store.lock().await;
        map.remove(&token).unwrap_or_default()
    };

    if frames.is_empty() {
        return Json(EnrollResponse {
            patient_token: token,
            embedding: vec![0.0f32; EMBEDDING_DIM],
            dimension: EMBEDDING_DIM,
            confidence: 0.0,
            samples_captured: 0,
        });
    }

    let dim = frames[0].len();
    let n = frames.len() as f32;
    let mut averaged = vec![0.0f32; dim];
    for frame in &frames {
        for (i, &v) in frame.iter().enumerate() {
            if i < dim {
                averaged[i] += v / n;
            }
        }
    }

    // Heuristic: assume the sensing loop targets ~10 fps.
    let confidence = (frames.len() as f32 / (duration as f32 * 10.0)).min(0.95);

    Json(EnrollResponse {
        patient_token: token,
        embedding: averaged,
        dimension: dim,
        confidence,
        samples_captured: frames.len(),
    })
}

/// Called by the main sensing loop each time a new AETHER pose embedding is
/// computed.  If `token` has an open enrollment window, the frame is buffered.
pub async fn push_enrollment_frame(store: &EnrollmentStore, token: &str, embedding: Vec<f32>) {
    let mut map = store.lock().await;
    if let Some(frames) = map.get_mut(token) {
        frames.push(embedding);
    }
}
