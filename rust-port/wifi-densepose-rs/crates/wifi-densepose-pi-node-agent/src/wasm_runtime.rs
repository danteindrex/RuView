use std::path::Path;

use anyhow::{bail, Result};
#[cfg(feature = "wasm-runtime")]
use anyhow::Context;

use crate::frame_encoder::WasmEvent;

#[derive(Debug, Clone)]
pub struct EdgeFrameContext {
    pub node_id: u8,
    pub sequence: u32,
    pub motion_energy: f32,
    pub presence_score: f32,
    pub timestamp_ms: u64,
}

#[derive(Debug)]
pub struct WasmRuntime {
    enabled: bool,
    module_id: u8,
    #[cfg(feature = "wasm-runtime")]
    engine: wasmtime::Engine,
    #[cfg(feature = "wasm-runtime")]
    module: Option<wasmtime::Module>,
}

impl WasmRuntime {
    pub fn disabled(module_id: u8) -> Self {
        Self {
            enabled: false,
            module_id,
            #[cfg(feature = "wasm-runtime")]
            engine: wasmtime::Engine::default(),
            #[cfg(feature = "wasm-runtime")]
            module: None,
        }
    }

    pub fn enabled_without_module(module_id: u8) -> Self {
        Self {
            enabled: true,
            module_id,
            #[cfg(feature = "wasm-runtime")]
            engine: wasmtime::Engine::default(),
            #[cfg(feature = "wasm-runtime")]
            module: None,
        }
    }

    #[cfg(feature = "wasm-runtime")]
    pub fn from_module(path: &Path, module_id: u8) -> Result<Self> {
        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::from_file(&engine, path)
            .with_context(|| format!("failed to load wasm module: {}", path.display()))?;
        Ok(Self {
            enabled: true,
            module_id,
            engine,
            module: Some(module),
        })
    }

    #[cfg(not(feature = "wasm-runtime"))]
    pub fn from_module(path: &Path, _module_id: u8) -> Result<Self> {
        let _ = path;
        bail!("wasm-runtime feature is disabled at compile time");
    }

    pub fn module_id(&self) -> u8 {
        self.module_id
    }

    pub fn on_frame(&mut self, context: &EdgeFrameContext) -> Vec<WasmEvent> {
        if !self.enabled {
            return Vec::new();
        }

        let mut events = Vec::new();
        if context.motion_energy > 1.0 {
            events.push(WasmEvent {
                event_type: 1,
                value: context.motion_energy,
            });
        }
        if context.presence_score > 0.5 {
            events.push(WasmEvent {
                event_type: 2,
                value: context.presence_score,
            });
        }
        if context.sequence.is_multiple_of(100) {
            events.push(WasmEvent {
                event_type: 3,
                value: (context.timestamp_ms % 1000) as f32 / 1000.0,
            });
        }
        events
    }
}
