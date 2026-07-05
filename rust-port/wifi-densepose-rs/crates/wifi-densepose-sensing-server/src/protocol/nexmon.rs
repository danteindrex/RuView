//! Native Nexmon CSI packet parser.
//!
//! Wire layout ground truth is `nexmon_csi/src/csi_extractor.c`
//! (`struct csi_udp_frame`, after the ethernet/IP/UDP headers):
//!
//! ```text
//! offset  field     type
//! 0..2    kk1       u16 LE magic 0x1111
//! 2       rssi      i8
//! 3       fc        u8  (frame control)
//! 4..10   SrcMac    [u8; 6]
//! 10..12  seqCnt    u16 LE
//! 12..14  csiconf   u16 LE
//! 14..16  chanspec  u16 LE
//! 16..18  chip      u16 LE
//! 18..    csi_values (i16 I, i16 Q pairs, LE)
//! ```

use crate::types::CsiFrame;

/// Header length of a Nexmon CSI UDP payload (`struct csi_udp_frame` minus
/// the ethernet/IP/UDP headers and the flexible CSI array).
pub const NEXMON_HEADER_LEN: usize = 18;

#[derive(Debug, Clone)]
pub struct NexmonPacket {
    pub rssi: i8,
    pub fc: u8,
    pub src_mac: [u8; 6],
    pub seq: u16,
    pub csiconf: u16,
    pub chanspec: u16,
    pub chip: u16,
    pub iq: Vec<(i16, i16)>,
}

pub fn parse_nexmon_payload(payload: &[u8]) -> Option<NexmonPacket> {
    // Header (18 bytes) plus at least one 4-byte I/Q pair.
    if payload.len() < NEXMON_HEADER_LEN + 4 {
        return None;
    }
    let magic = u16::from_le_bytes([payload[0], payload[1]]);
    if magic != 0x1111 {
        return None;
    }

    let rssi = payload[2] as i8;
    let fc = payload[3];
    let mut src_mac = [0u8; 6];
    src_mac.copy_from_slice(&payload[4..10]);
    let seq = u16::from_le_bytes([payload[10], payload[11]]);
    let csiconf = u16::from_le_bytes([payload[12], payload[13]]);
    let chanspec = u16::from_le_bytes([payload[14], payload[15]]);
    let chip = u16::from_le_bytes([payload[16], payload[17]]);

    let csi = &payload[NEXMON_HEADER_LEN..];
    let n_sc = csi.len() / 4;
    if n_sc == 0 {
        return None;
    }

    let mut iq = Vec::with_capacity(n_sc);
    for i in 0..n_sc {
        let off = i * 4;
        let re = i16::from_le_bytes([csi[off], csi[off + 1]]);
        let im = i16::from_le_bytes([csi[off + 2], csi[off + 3]]);
        iq.push((re, im));
    }

    Some(NexmonPacket {
        rssi,
        fc,
        src_mac,
        seq,
        csiconf,
        chanspec,
        chip,
        iq,
    })
}

pub fn parse_nexmon_as_esp32_frame(payload: &[u8], node_base: u8) -> Option<CsiFrame> {
    let pkt = parse_nexmon_payload(payload)?;
    // csiconf carries the RX core / spatial-stream config; the core index is
    // stable per Pi, so one Pi maps to one node id.
    let core = (pkt.csiconf & 0x7) as u8;
    let node_id = node_base.wrapping_add(core);
    let freq_mhz = chanspec_to_freq(pkt.chanspec).unwrap_or(2437);

    let n_sub = pkt.iq.len();
    let mut amplitudes = Vec::with_capacity(n_sub);
    let mut phases = Vec::with_capacity(n_sub);
    for (i, q) in &pkt.iq {
        let i_f = *i as f64;
        let q_f = *q as f64;
        amplitudes.push((i_f * i_f + q_f * q_f).sqrt());
        phases.push(q_f.atan2(i_f));
    }

    Some(CsiFrame {
        magic: 0xC511_0001,
        node_id,
        n_antennas: 1,
        n_subcarriers: n_sub as u16,
        freq_mhz,
        sequence: pkt.seq as u32,
        rssi: pkt.rssi,
        noise_floor: -92,
        amplitudes,
        phases,
    })
}

fn chanspec_to_freq(chanspec: u16) -> Option<u16> {
    let ch = chanspec & 0x00ff;
    if (1..=14).contains(&ch) {
        return Some(2407 + 5 * ch);
    }
    if (30..=196).contains(&ch) {
        return Some(5000 + 5 * ch);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden packet built field-by-field from `struct csi_udp_frame` in
    /// nexmon_csi/src/csi_extractor.c:135-146 (post-UDP payload only).
    fn golden_packet() -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0x1111u16.to_le_bytes()); // kk1
        p.push((-42i8) as u8); // rssi
        p.push(0x88); // fc
        p.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01]); // SrcMac
        p.extend_from_slice(&0x1234u16.to_le_bytes()); // seqCnt
        p.extend_from_slice(&0x0002u16.to_le_bytes()); // csiconf (core 2)
        p.extend_from_slice(&0x1006u16.to_le_bytes()); // chanspec (channel 6)
        p.extend_from_slice(&0x006Au16.to_le_bytes()); // chip
        // Two I/Q pairs: (100, -50), (-3, 7)
        p.extend_from_slice(&100i16.to_le_bytes());
        p.extend_from_slice(&(-50i16).to_le_bytes());
        p.extend_from_slice(&(-3i16).to_le_bytes());
        p.extend_from_slice(&7i16.to_le_bytes());
        p
    }

    #[test]
    fn parses_golden_packet_fields() {
        let pkt = parse_nexmon_payload(&golden_packet()).expect("golden packet must parse");
        assert_eq!(pkt.rssi, -42);
        assert_eq!(pkt.fc, 0x88);
        assert_eq!(pkt.src_mac, [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01]);
        assert_eq!(pkt.seq, 0x1234);
        assert_eq!(pkt.csiconf, 0x0002);
        assert_eq!(pkt.chanspec, 0x1006);
        assert_eq!(pkt.chip, 0x006A);
        assert_eq!(pkt.iq, vec![(100, -50), (-3, 7)]);
    }

    #[test]
    fn golden_packet_to_csi_frame_uses_real_rssi_and_stable_node_id() {
        let frame = parse_nexmon_as_esp32_frame(&golden_packet(), 10).expect("must convert");
        assert_eq!(frame.rssi, -42, "RSSI must come from the header byte, not an IQ estimate");
        assert_eq!(frame.node_id, 12, "node_id = node_base + core from csiconf");
        assert_eq!(frame.n_subcarriers, 2);
        assert_eq!(frame.sequence, 0x1234);
        assert_eq!(frame.freq_mhz, 2437);
        let a0 = (100.0f64 * 100.0 + 2500.0).sqrt();
        assert!((frame.amplitudes[0] - a0).abs() < 1e-9);
    }

    #[test]
    fn node_id_stable_across_sequence_numbers() {
        // Regression: the old parser read seqCnt as the core/ss config, so a
        // single Pi scattered into up to 8 phantom node ids as seq counted up.
        let mut ids = std::collections::HashSet::new();
        for seq in 0u16..64 {
            let mut p = golden_packet();
            p[10..12].copy_from_slice(&seq.to_le_bytes());
            let frame = parse_nexmon_as_esp32_frame(&p, 10).unwrap();
            ids.insert(frame.node_id);
        }
        assert_eq!(ids.len(), 1, "one Pi must map to exactly one node id");
    }

    #[test]
    fn rejects_short_and_wrong_magic() {
        assert!(parse_nexmon_payload(&golden_packet()[..20]).is_none(), "min length is 22");
        let mut bad = golden_packet();
        bad[0] = 0x22;
        assert!(parse_nexmon_payload(&bad).is_none());
    }
}
