use std::time::Duration;

use wifi_densepose_pi_node_agent::edge_dsp::{process_frame, EdgeDspState};
use wifi_densepose_pi_node_agent::frame_encoder::{
    decode_magic, encode_fused_vitals_packet, encode_raw_frame, encode_wasm_v2_packet,
    MAGIC_FUSED_VITALS, MAGIC_RAW_FRAME, MAGIC_VITALS, MAGIC_WASM_V2,
};
use wifi_densepose_pi_node_agent::mmwave::{fuse_with_mmwave, MmwaveState};
use wifi_densepose_pi_node_agent::nexmon_capture::{nexmon_to_raw_frame, parse_nexmon_payload};
use wifi_densepose_pi_node_agent::wasm_runtime::{EdgeFrameContext, WasmRuntime};

fn fixture_nexmon_pkt() -> Vec<u8> {
    let mut buf = vec![0u8; 16 + 64 * 4];
    buf[0..2].copy_from_slice(&0x1111u16.to_le_bytes());
    buf[8..10].copy_from_slice(&1234u16.to_le_bytes());
    buf[10..12].copy_from_slice(&0u16.to_le_bytes());
    buf[12..14].copy_from_slice(&6u16.to_le_bytes());
    buf[14..16].copy_from_slice(&0x4355u16.to_le_bytes());
    for i in 0..64 {
        let off = 16 + i * 4;
        let re = (i as i16) - 32;
        let im = 32 - (i as i16);
        buf[off..off + 2].copy_from_slice(&re.to_le_bytes());
        buf[off + 2..off + 4].copy_from_slice(&im.to_le_bytes());
    }
    buf
}

#[tokio::test]
async fn agent_converts_nexmon_to_raw_frame_packet() {
    let pkt = parse_nexmon_payload(&fixture_nexmon_pkt()).expect("nexmon");
    let frame = nexmon_to_raw_frame(&pkt, 10, -55, -92);
    let out = encode_raw_frame(&frame);
    assert_eq!(
        u32::from_le_bytes(out[0..4].try_into().expect("magic bytes")),
        MAGIC_RAW_FRAME
    );
    assert_eq!(decode_magic(&out), Some(MAGIC_RAW_FRAME));
    assert_eq!(frame.n_subcarriers, 64);
}

#[tokio::test]
async fn emits_vitals_and_feature_packets_every_second() {
    let pkt = parse_nexmon_payload(&fixture_nexmon_pkt()).expect("nexmon");
    let frame = nexmon_to_raw_frame(&pkt, 10, -55, -92);
    let mut dsp = EdgeDspState::new(2);

    let out0 = process_frame(&mut dsp, &frame, 0);
    assert!(out0.vitals.is_some());
    assert!(out0.feature.is_some());
    assert!(out0.compressed.is_some());

    let out1 = process_frame(&mut dsp, &frame, 500);
    assert!(out1.vitals.is_none());

    let out2 = process_frame(&mut dsp, &frame, 1_100);
    assert!(out2.vitals.is_some());
    assert!(out2.feature.is_some());
    let vitals = out2.vitals.expect("vitals");
    let packet = wifi_densepose_pi_node_agent::frame_encoder::encode_vitals_packet(&vitals);
    assert_eq!(decode_magic(&packet), Some(MAGIC_VITALS));
}

#[test]
fn fusion_packet_is_emitted_when_mmwave_present() {
    let pkt = parse_nexmon_payload(&fixture_nexmon_pkt()).expect("nexmon");
    let frame = nexmon_to_raw_frame(&pkt, 10, -55, -92);
    let mut dsp = EdgeDspState::new(2);
    let out = process_frame(&mut dsp, &frame, 1_000);
    let vitals = out.vitals.expect("vitals");

    let mmwave = MmwaveState {
        presence_score: 0.8,
        motion_energy: 0.5,
        distance_m: 1.4,
        fall_detected: false,
        n_persons: 2,
    };
    let fused = fuse_with_mmwave(&vitals, &mmwave);
    let pkt = encode_fused_vitals_packet(&fused);
    assert_eq!(decode_magic(&pkt), Some(MAGIC_FUSED_VITALS));
    assert!(pkt.len() >= 32);
}

#[tokio::test]
async fn wasm_runtime_emits_v2_wasm_packet() {
    let mut runtime = WasmRuntime::enabled_without_module(7);
    let ctx = EdgeFrameContext {
        node_id: 10,
        sequence: 100,
        motion_energy: 1.3,
        presence_score: 0.7,
        timestamp_ms: Duration::from_secs(1).as_millis() as u64,
    };
    let events = runtime.on_frame(&ctx);
    assert!(!events.is_empty());
    let pkt = encode_wasm_v2_packet(ctx.node_id, runtime.module_id(), &events);
    assert_eq!(decode_magic(&pkt), Some(MAGIC_WASM_V2));
}
