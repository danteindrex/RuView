use wifi_densepose_sensing_server::protocol::esp32_legacy::parse_esp32_frame;

fn make_real_esp32_frame() -> Vec<u8> {
    let n_sub = 64u16;
    let n_antennas = 1u8;
    let n_pairs = n_sub as usize * n_antennas as usize;
    let mut buf = vec![0u8; 20 + n_pairs * 2];

    buf[0..4].copy_from_slice(&0xC511_0001u32.to_le_bytes());
    buf[4] = 9; // node_id
    buf[5] = n_antennas;
    buf[6..8].copy_from_slice(&n_sub.to_le_bytes());
    buf[8..12].copy_from_slice(&2437u32.to_le_bytes());
    buf[12..16].copy_from_slice(&42u32.to_le_bytes());
    buf[16] = (-55i8) as u8;
    buf[17] = (-92i8) as u8;

    for i in 0..n_pairs {
        buf[20 + i * 2] = (i as i8).wrapping_sub(8) as u8;
        buf[20 + i * 2 + 1] = (8i8).wrapping_sub(i as i8) as u8;
    }
    buf
}

#[test]
fn parse_raw_frame_uses_u16_subcarriers_and_u32_freq_offsets() {
    let buf = make_real_esp32_frame();
    let f = parse_esp32_frame(&buf).expect("frame");
    assert_eq!(f.node_id, 9);
    assert_eq!(f.n_subcarriers, 64);
    assert_eq!(f.freq_mhz, 2437);
    assert_eq!(f.sequence, 42);
    assert_eq!(f.rssi, -55);
    assert_eq!(f.noise_floor, -92);
    assert_eq!(f.amplitudes.len(), 64);
}

