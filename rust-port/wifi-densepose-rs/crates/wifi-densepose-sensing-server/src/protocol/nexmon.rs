//! Native Nexmon CSI packet parser.

use crate::types::CsiFrame;

#[derive(Debug, Clone)]
pub struct NexmonPacket {
    pub seq: u16,
    pub core_ss: u16,
    pub chanspec: u16,
    pub chip: u16,
    pub iq: Vec<(i16, i16)>,
}

pub fn parse_nexmon_payload(payload: &[u8]) -> Option<NexmonPacket> {
    if payload.len() < 20 {
        return None;
    }
    let magic = u16::from_le_bytes([payload[0], payload[1]]);
    if magic != 0x1111 {
        return None;
    }

    let seq = u16::from_le_bytes([payload[8], payload[9]]);
    let core_ss = u16::from_le_bytes([payload[10], payload[11]]);
    let chanspec = u16::from_le_bytes([payload[12], payload[13]]);
    let chip = u16::from_le_bytes([payload[14], payload[15]]);

    let csi = &payload[16..];
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
        seq,
        core_ss,
        chanspec,
        chip,
        iq,
    })
}

pub fn parse_nexmon_as_esp32_frame(payload: &[u8], node_base: u8) -> Option<CsiFrame> {
    let pkt = parse_nexmon_payload(payload)?;
    let core = (pkt.core_ss & 0x7) as u8;
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

    // Estimate RSSI from I/Q power rather than hardcoding.
    let rssi = estimate_rssi_from_iq(&pkt.iq);

    Some(CsiFrame {
        magic: 0xC511_0001,
        node_id,
        n_antennas: 1,
        n_subcarriers: n_sub as u16,
        freq_mhz,
        sequence: pkt.seq as u32,
        rssi,
        noise_floor: -92,
        amplitudes,
        phases,
    })
}

/// Estimate RSSI from Nexmon I/Q data (dBm)—a rough power-based approximation
/// since the raw Nexmon payload doesn't always include RSSI.
fn estimate_rssi_from_iq(iq: &[(i16, i16)]) -> i8 {
    if iq.is_empty() {
        return -55; // safe fallback
    }
    let power: f64 = iq.iter()
        .map(|(i, q)| (*i as f64).powi(2) + (*q as f64).powi(2))
        .sum::<f64>() / iq.len() as f64;
    if power < 1.0 {
        return -90;
    }
    let rssi_est = 10.0 * power.log10() - 40.0;
    rssi_est.clamp(-127.0, 0.0) as i8
}

fn chanspec_to_freq(chanspec: u16) -> Option<u16> {
    let ch = (chanspec & 0x00ff) as u16;
    if (1..=14).contains(&ch) {
        return Some(2407 + 5 * ch);
    }
    if (30..=196).contains(&ch) {
        return Some(5000 + 5 * ch);
    }
    None
}
