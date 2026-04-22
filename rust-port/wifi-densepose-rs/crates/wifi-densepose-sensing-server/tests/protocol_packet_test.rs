use wifi_densepose_sensing_server::protocol::packet::{decode_packet, DecodedPacket, Magic};

fn make_feature_packet() -> Vec<u8> {
    let mut buf = vec![0u8; 48];
    buf[0..4].copy_from_slice(&(Magic::Feature as u32).to_le_bytes());
    buf[4] = 7; // node_id
    buf[6..8].copy_from_slice(&11u16.to_le_bytes()); // seq
    buf[8..16].copy_from_slice(&123_456_i64.to_le_bytes());
    let mut off = 16usize;
    for i in 0..8 {
        let f = (i as f32) * 0.1;
        buf[off..off + 4].copy_from_slice(&f.to_le_bytes());
        off += 4;
    }
    buf
}

fn make_compressed_packet() -> Vec<u8> {
    let payload = vec![0xAA, 0x02, 0x00, 0x01];
    let mut buf = vec![0u8; 10 + payload.len()];
    buf[0..4].copy_from_slice(&(Magic::Compressed as u32).to_le_bytes());
    buf[4] = 3; // node_id
    buf[5] = 6; // channel
    buf[6..8].copy_from_slice(&64u16.to_le_bytes()); // original
    buf[8..10].copy_from_slice(&(payload.len() as u16).to_le_bytes()); // compressed
    buf[10..].copy_from_slice(&payload);
    buf
}

fn make_wasm_v2_packet() -> Vec<u8> {
    let mut buf = vec![0u8; 13];
    buf[0..4].copy_from_slice(&(Magic::WasmOutputV2 as u32).to_le_bytes());
    buf[4] = 2; // node_id
    buf[5] = 1; // module_id
    buf[6..8].copy_from_slice(&1u16.to_le_bytes()); // event_count
    buf[8] = 9; // event type
    buf[9..13].copy_from_slice(&1.25f32.to_le_bytes());
    buf
}

#[test]
fn packet_magic_values_are_unique() {
    let all = vec![
        Magic::RawFrame as u32,
        Magic::Vitals as u32,
        Magic::Feature as u32,
        Magic::FusedVitals as u32,
        Magic::Compressed as u32,
        Magic::WasmOutputV2 as u32,
    ];
    let mut uniq = all.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(all.len(), uniq.len());
}

#[test]
fn decode_supports_feature_compressed_and_wasm_v2_packets() {
    let feature = decode_packet(&make_feature_packet());
    assert!(matches!(feature, Some(DecodedPacket::EdgeFeature(_))));

    let compressed = decode_packet(&make_compressed_packet());
    assert!(matches!(compressed, Some(DecodedPacket::Compressed(_))));

    let wasm = decode_packet(&make_wasm_v2_packet());
    assert!(matches!(wasm, Some(DecodedPacket::WasmOutput(_))));
}

