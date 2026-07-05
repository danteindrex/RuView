use wifi_densepose_sensing_server::protocol::nexmon::{
    parse_nexmon_as_esp32_frame, parse_nexmon_payload,
};

/// Fixture built per the real nexmon_csi firmware header
/// (nexmon_csi/src/csi_extractor.c:135-146):
/// magic u16 [0..2], rssi i8 [2], fc u8 [3], src MAC [4..10],
/// seqCnt u16 LE [10..12], csiconf [12..14], chanspec [14..16],
/// chip [16..18], I/Q pairs from offset 18.
fn fixture_nexmon_pkt() -> Vec<u8> {
    let mut buf = vec![0u8; 18 + 64 * 4];
    buf[0..2].copy_from_slice(&0x1111u16.to_le_bytes());
    buf[2] = (-42i8) as u8; // rssi
    buf[3] = 0x88; // frame control
    buf[4..10].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]); // src mac
    buf[10..12].copy_from_slice(&1234u16.to_le_bytes()); // seq
    buf[12..14].copy_from_slice(&0u16.to_le_bytes()); // csiconf
    buf[14..16].copy_from_slice(&6u16.to_le_bytes()); // chanspec low byte channel 6
    buf[16..18].copy_from_slice(&0x4355u16.to_le_bytes()); // chip id fixture

    for i in 0..64 {
        let off = 18 + i * 4;
        let re = (i as i16) - 32;
        let im = 32 - (i as i16);
        buf[off..off + 2].copy_from_slice(&re.to_le_bytes());
        buf[off + 2..off + 4].copy_from_slice(&im.to_le_bytes());
    }
    buf
}

#[test]
fn parse_nexmon_0x1111_payload_extracts_iq_and_metadata() {
    let pkt = parse_nexmon_payload(&fixture_nexmon_pkt()).expect("nexmon");
    assert_eq!(pkt.seq, 1234);
    assert_eq!(pkt.rssi, -42);
    assert_eq!(pkt.fc, 0x88);
    assert_eq!(pkt.src_mac, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
    assert_eq!(pkt.chanspec & 0x00ff, 6);
    assert_eq!(pkt.iq.len(), 64);
}

#[test]
fn parse_nexmon_payload_converts_to_esp32_frame_for_pipeline() {
    let frame = parse_nexmon_as_esp32_frame(&fixture_nexmon_pkt(), 10).expect("frame");
    assert_eq!(frame.magic, 0xC511_0001);
    assert_eq!(frame.node_id, 10);
    assert_eq!(frame.sequence, 1234);
    assert_eq!(frame.freq_mhz, 2437);
    assert_eq!(frame.amplitudes.len(), 64);
    assert_eq!(frame.rssi, -42);
}
