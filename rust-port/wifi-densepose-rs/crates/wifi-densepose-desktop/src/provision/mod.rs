//! Native provisioning support: ESP-IDF NVS binary generation without a
//! Python/ESP-IDF dependency on the operator's machine.
//!
//! See [`nvs_gen`] for the byte-format implementation and the golden test
//! that proves parity with ESP-IDF's `esp_idf_nvs_partition_gen`.

pub mod nvs_gen;

pub use nvs_gen::{generate_nvs_csi_cfg, Esp32NvsConfig, NVS_PARTITION_SIZE};
