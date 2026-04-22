use std::collections::VecDeque;

use crate::frame_encoder::{EdgeVitals, FusedVitals};

#[derive(Debug, Clone)]
pub struct MmwaveState {
    pub presence_score: f32,
    pub motion_energy: f32,
    pub distance_m: f32,
    pub fall_detected: bool,
    pub n_persons: u8,
}

impl Default for MmwaveState {
    fn default() -> Self {
        Self {
            presence_score: 0.0,
            motion_energy: 0.0,
            distance_m: 0.0,
            fall_detected: false,
            n_persons: 0,
        }
    }
}

pub trait MmwaveReader: Send {
    fn poll(&mut self) -> Option<MmwaveState>;
}

#[derive(Debug, Default)]
pub struct MockMmwaveReader {
    queue: VecDeque<MmwaveState>,
}

impl MockMmwaveReader {
    pub fn push(&mut self, value: MmwaveState) {
        self.queue.push_back(value);
    }
}

impl MmwaveReader for MockMmwaveReader {
    fn poll(&mut self) -> Option<MmwaveState> {
        self.queue.pop_front()
    }
}

#[derive(Debug, Default)]
pub struct Ld2410Reader {
    pub latest: Option<MmwaveState>,
}

impl MmwaveReader for Ld2410Reader {
    fn poll(&mut self) -> Option<MmwaveState> {
        self.latest.take()
    }
}

#[derive(Debug, Default)]
pub struct Mr60bha2Reader {
    pub latest: Option<MmwaveState>,
}

impl MmwaveReader for Mr60bha2Reader {
    fn poll(&mut self) -> Option<MmwaveState> {
        self.latest.take()
    }
}

pub fn fuse_with_mmwave(vitals: &EdgeVitals, mmwave: &MmwaveState) -> FusedVitals {
    FusedVitals {
        node_id: vitals.node_id,
        presence: vitals.presence || mmwave.presence_score > 0.3,
        fall_detected: vitals.fall_detected || mmwave.fall_detected,
        motion: vitals.motion || mmwave.motion_energy > 0.2,
        breathing_rate_bpm: vitals.breathing_rate_bpm,
        heartrate_bpm: vitals.heartrate_bpm,
        rssi: vitals.rssi,
        n_persons: vitals.n_persons.max(mmwave.n_persons),
        motion_energy: vitals.motion_energy.max(mmwave.motion_energy),
        presence_score: vitals.presence_score.max(mmwave.presence_score),
        timestamp_ms: vitals.timestamp_ms,
    }
}
