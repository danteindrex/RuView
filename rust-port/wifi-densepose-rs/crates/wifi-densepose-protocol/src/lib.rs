use serde::{Deserialize, Serialize};

pub const MAGIC_RAW_FRAME: u32 = 0xC511_0001;
pub const MAGIC_VITALS: u32 = 0xC511_0002;
pub const MAGIC_FEATURE: u32 = 0xC511_0003;
pub const MAGIC_FUSED_VITALS: u32 = 0xC511_0004;
pub const MAGIC_COMPRESSED: u32 = 0xC511_0005;
pub const MAGIC_WASM_V2: u32 = 0xC511_0006;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u32)]
pub enum Magic {
    RawFrame = MAGIC_RAW_FRAME,
    Vitals = MAGIC_VITALS,
    Feature = MAGIC_FEATURE,
    FusedVitals = MAGIC_FUSED_VITALS,
    Compressed = MAGIC_COMPRESSED,
    WasmOutputV2 = MAGIC_WASM_V2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmEvent {
    pub event_type: u8,
    pub value: f32,
}
