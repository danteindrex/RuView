//! Canonical packet identifiers and top-level decoding.

use crate::types::{Esp32Frame, Esp32VitalsPacket, WasmOutputPacket};

use super::esp32_legacy::{
    parse_esp32_compressed_packet, parse_esp32_feature_packet, parse_esp32_frame,
    parse_esp32_vitals_or_fused, parse_esp32_wasm_output, Esp32CompressedPacket,
    Esp32FeaturePacket,
};

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Magic {
    RawFrame = 0xC511_0001,
    Vitals = 0xC511_0002,
    Feature = 0xC511_0003,
    FusedVitals = 0xC511_0004,
    Compressed = 0xC511_0005,
    WasmOutputV2 = 0xC511_0006,
}

#[derive(Debug, Clone)]
pub enum DecodedPacket {
    RawFrame(Esp32Frame),
    EdgeVitals(Esp32VitalsPacket),
    EdgeFeature(Esp32FeaturePacket),
    Compressed(Esp32CompressedPacket),
    WasmOutput(WasmOutputPacket),
}

pub fn decode_packet(buf: &[u8]) -> Option<DecodedPacket> {
    if buf.len() < 4 {
        return None;
    }
    let m = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);

    match m {
        x if x == Magic::Vitals as u32 => {
            parse_esp32_vitals_or_fused(buf).map(DecodedPacket::EdgeVitals)
        }
        x if x == Magic::FusedVitals as u32 => {
            if let Some(w) = parse_esp32_wasm_output(buf) {
                return Some(DecodedPacket::WasmOutput(w));
            }
            parse_esp32_vitals_or_fused(buf).map(DecodedPacket::EdgeVitals)
        }
        x if x == Magic::Feature as u32 => {
            parse_esp32_feature_packet(buf).map(DecodedPacket::EdgeFeature)
        }
        x if x == Magic::Compressed as u32 => {
            parse_esp32_compressed_packet(buf).map(DecodedPacket::Compressed)
        }
        x if x == Magic::WasmOutputV2 as u32 => {
            parse_esp32_wasm_output(buf).map(DecodedPacket::WasmOutput)
        }
        x if x == Magic::RawFrame as u32 => parse_esp32_frame(buf).map(DecodedPacket::RawFrame),
        _ => {
            // Try Nexmon CSI format (magic 0x1111) as last resort.
            super::nexmon::parse_nexmon_as_esp32_frame(buf, 10)
                .map(DecodedPacket::RawFrame)
        }
    }
}
