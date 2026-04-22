use crate::frame_encoder::RawFrame;

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
    for index in 0..n_sc {
        let offset = index * 4;
        let i = i16::from_le_bytes([csi[offset], csi[offset + 1]]);
        let q = i16::from_le_bytes([csi[offset + 2], csi[offset + 3]]);
        iq.push((i, q));
    }

    Some(NexmonPacket {
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

fn estimate_rssi_from_iq(iq: &[(i16, i16)], default_rssi: i8) -> i8 {
    if iq.is_empty() {
        return default_rssi;
    }
    let mean_power = iq
        .iter()
        .map(|(i, q)| (*i as f64).powi(2) + (*q as f64).powi(2))
        .sum::<f64>()
        / iq.len() as f64;
    if mean_power < 1.0 {
        return default_rssi;
    }
    let rssi = (10.0 * mean_power.log10() - 40.0).clamp(-127.0, 0.0);
    rssi as i8
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

    RawFrame {
        node_id,
        n_antennas: 1,
        n_subcarriers: iq.len() as u16,
        freq_mhz,
        sequence: pkt.seq as u32,
        rssi: estimate_rssi_from_iq(&pkt.iq, default_rssi),
        noise_floor,
        iq,
    }
}
