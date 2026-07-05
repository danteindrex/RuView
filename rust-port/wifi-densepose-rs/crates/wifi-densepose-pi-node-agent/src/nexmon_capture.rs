use crate::frame_encoder::RawFrame;

/// UDP payload layout emitted by the nexmon_csi firmware patch, per
/// `struct csi_udp_frame` in `nexmon_csi/src/csi_extractor.c:135-146`
/// (offsets are after the ethernet/IP/UDP headers stripped by the socket):
///
/// | Offset   | Field    | Type          |
/// |----------|----------|---------------|
/// | [0..2)   | kk1      | u16 LE magic 0x1111 |
/// | [2]      | rssi     | i8            |
/// | [3]      | fc       | u8 (frame control) |
/// | [4..10)  | SrcMac   | [u8; 6]       |
/// | [10..12) | seqCnt   | u16 LE        |
/// | [12..14) | csiconf  | u16 LE (core / spatial-stream config) |
/// | [14..16) | chanspec | u16 LE        |
/// | [16..18) | chip     | u16 LE        |
/// | [18..]   | csi_values | interleaved i16 LE I/Q pairs |
const NEXMON_HEADER_LEN: usize = 18;
/// Header plus at least one 4-byte I/Q sample.
const NEXMON_MIN_LEN: usize = NEXMON_HEADER_LEN + 4;

#[derive(Debug, Clone)]
pub struct NexmonPacket {
    /// Instantaneous RSSI computed by the firmware (0 until first update).
    pub rssi: i8,
    /// 802.11 frame control byte of the sampled frame.
    pub fc: u8,
    /// Source MAC address of the transmitter the CSI was captured from.
    pub source_mac: [u8; 6],
    pub seq: u16,
    /// `csiconf` field from firmware; low bits carry core / spatial stream.
    pub core_ss: u16,
    pub chanspec: u16,
    pub chip: u16,
    pub iq: Vec<(i16, i16)>,
}

pub fn parse_nexmon_payload(payload: &[u8]) -> Option<NexmonPacket> {
    if payload.len() < NEXMON_MIN_LEN {
        return None;
    }

    let magic = u16::from_le_bytes([payload[0], payload[1]]);
    if magic != 0x1111 {
        return None;
    }

    let rssi = payload[2] as i8;
    let fc = payload[3];
    let mut source_mac = [0u8; 6];
    source_mac.copy_from_slice(&payload[4..10]);
    let seq = u16::from_le_bytes([payload[10], payload[11]]);
    let core_ss = u16::from_le_bytes([payload[12], payload[13]]);
    let chanspec = u16::from_le_bytes([payload[14], payload[15]]);
    let chip = u16::from_le_bytes([payload[16], payload[17]]);

    let csi = &payload[NEXMON_HEADER_LEN..];
    let n_sc = csi.len() / 4;
    if n_sc == 0 {
        return None;
    }

    let mut iq = Vec::with_capacity(n_sc);
    for index in 0..n_sc {
        let offset = index * 4;
        let i = i16::from_le_bytes([csi[offset], csi[offset + 1]]);
        let q = i16::from_le_bytes([csi[offset + 2], csi[offset + 3]]);
        iq.push((i, q));
    }

    Some(NexmonPacket {
        rssi,
        fc,
        source_mac,
        seq,
        core_ss,
        chanspec,
        chip,
        iq,
    })
}

pub fn core_from_core_ss(core_ss: u16) -> u8 {
    (core_ss & 0x7) as u8
}

pub fn chanspec_to_freq(chanspec: u16) -> Option<u16> {
    let ch = chanspec & 0x00ff;
    if (1..=14).contains(&ch) {
        return Some(2407 + 5 * ch);
    }
    if (30..=196).contains(&ch) {
        return Some(5000 + 5 * ch);
    }
    None
}

fn scale_iq_to_i8(iq: &[(i16, i16)]) -> Vec<(i8, i8)> {
    let peak = iq
        .iter()
        .flat_map(|(i, q)| [i.abs() as i32, q.abs() as i32])
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let scale = if peak > 120.0 { peak / 120.0 } else { 1.0 };

    iq.iter()
        .map(|(i, q)| {
            let i = ((*i as f32) / scale).round().clamp(-127.0, 127.0) as i8;
            let q = ((*q as f32) / scale).round().clamp(-127.0, 127.0) as i8;
            (i, q)
        })
        .collect()
}

pub fn nexmon_to_raw_frame(
    pkt: &NexmonPacket,
    node_base: u8,
    default_rssi: i8,
    noise_floor: i8,
) -> RawFrame {
    let core = core_from_core_ss(pkt.core_ss);
    let node_id = node_base.wrapping_add(core);
    let freq_mhz = chanspec_to_freq(pkt.chanspec).unwrap_or(2437);
    let iq = scale_iq_to_i8(&pkt.iq);
    // The firmware reports rssi == 0 until its `last_rssi` is first updated
    // (csi_extractor.c); fall back to the configured default in that case.
    let rssi = if pkt.rssi != 0 { pkt.rssi } else { default_rssi };

    RawFrame {
        node_id,
        n_antennas: 1,
        n_subcarriers: iq.len() as u16,
        freq_mhz,
        sequence: pkt.seq as u32,
        rssi,
        noise_floor,
        iq,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a UDP payload byte-for-byte per `struct csi_udp_frame` in
    /// `nexmon_csi/src/csi_extractor.c` (fields after the eth/IP/UDP headers).
    fn golden_packet() -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0x1111u16.to_le_bytes()); // kk1 magic
        p.push((-42i8) as u8); // rssi
        p.push(0x88); // fc (QoS data)
        p.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55]); // SrcMac
        p.extend_from_slice(&0x1234u16.to_le_bytes()); // seqCnt
        p.extend_from_slice(&0x0002u16.to_le_bytes()); // csiconf (core 2)
        p.extend_from_slice(&0xd024u16.to_le_bytes()); // chanspec (channel 36)
        p.extend_from_slice(&0x4345u16.to_le_bytes()); // chip
        for (i, q) in [(100i16, -50i16), (-3, 7)] {
            p.extend_from_slice(&i.to_le_bytes());
            p.extend_from_slice(&q.to_le_bytes());
        }
        p
    }

    #[test]
    fn parses_golden_packet_per_csi_extractor_layout() {
        let pkt = parse_nexmon_payload(&golden_packet()).expect("golden packet must parse");
        assert_eq!(pkt.rssi, -42);
        assert_eq!(pkt.fc, 0x88);
        assert_eq!(pkt.source_mac, [0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        assert_eq!(pkt.seq, 0x1234);
        assert_eq!(pkt.core_ss, 0x0002);
        assert_eq!(pkt.chanspec, 0xd024);
        assert_eq!(pkt.chip, 0x4345);
        assert_eq!(pkt.iq, vec![(100, -50), (-3, 7)]);
    }

    #[test]
    fn rejects_short_and_bad_magic_packets() {
        let golden = golden_packet();
        assert!(parse_nexmon_payload(&golden[..NEXMON_MIN_LEN - 1]).is_none());
        let mut bad_magic = golden.clone();
        bad_magic[0] = 0x22;
        assert!(parse_nexmon_payload(&bad_magic).is_none());
    }

    #[test]
    fn raw_frame_uses_firmware_rssi_and_correct_alignment() {
        let pkt = parse_nexmon_payload(&golden_packet()).unwrap();
        let frame = nexmon_to_raw_frame(&pkt, 10, -55, -92);
        assert_eq!(frame.rssi, -42); // real firmware rssi, not an estimate
        assert_eq!(frame.node_id, 12); // node_base 10 + core 2
        assert_eq!(frame.freq_mhz, 5180); // chanspec channel 36
        assert_eq!(frame.sequence, 0x1234);
        assert_eq!(frame.n_subcarriers, 2);
        assert_eq!(frame.noise_floor, -92);
        // IQ within i8 range passes through unscaled and correctly aligned.
        assert_eq!(frame.iq, vec![(100, -50), (-3, 7)]);
    }

    #[test]
    fn raw_frame_falls_back_to_default_rssi_when_firmware_reports_zero() {
        let mut payload = golden_packet();
        payload[2] = 0; // firmware last_rssi not yet updated
        let pkt = parse_nexmon_payload(&payload).unwrap();
        let frame = nexmon_to_raw_frame(&pkt, 10, -55, -92);
        assert_eq!(frame.rssi, -55);
    }
}
