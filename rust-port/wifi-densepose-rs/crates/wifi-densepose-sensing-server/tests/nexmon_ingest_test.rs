use wifi_densepose_sensing_server::protocol::nexmon::{
    parse_nexmon_as_esp32_frame, parse_nexmon_payload,
};

fn fixture_nexmon_pkt() -> Vec<u8> {
    let mut buf = vec![0u8; 16 + 64 * 4];
    buf[0..2].copy_from_slice(&0x1111u16.to_le_bytes());
    // src mac bytes [2..8] left as zero for fixture
    buf[8..10].copy_from_slice(&1234u16.to_le_bytes()); // seq
    buf[10..12].copy_from_slice(&0u16.to_le_bytes()); // core_ss (core 0)
    buf[12..14].copy_from_slice(&6u16.to_le_bytes()); // chanspec low byte channel 6
    buf[14..16].copy_from_slice(&0x4355u16.to_le_bytes()); // chip id fixture

    for i in 0..64 {
        let off = 16 + i * 4;
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
    assert_eq!(pkt.core_ss, 0);
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
}

