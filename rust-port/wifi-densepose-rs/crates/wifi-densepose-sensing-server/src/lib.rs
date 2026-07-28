//! WiFi-DensePose Sensing Server library.
//!
//! This crate provides:
//! - Vital sign detection from WiFi CSI amplitude data
//! - RVF (RuVector Format) binary container for model weights

pub mod vital_signs;
pub mod rvf_container;
pub mod rvf_pipeline;
pub mod graph_transformer;
#[allow(dead_code)]
pub mod trainer;
pub mod dataset;
pub mod sona;
pub mod sparse_inference;
#[allow(dead_code)]
pub mod embedding;
pub mod protocol;
pub mod types;
pub mod adaptive_classifier;
pub mod csi;
pub mod field_bridge;
/// Interoperability standards adapters (medical + physical security):
/// IEC 60601-1-8 alarms, HL7 v2 / IHE PCD-01, SIA DC-09, ONVIF.
pub mod standards;
