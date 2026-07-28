//! ANSI/SIA DC-09 alarm-transport message encoder.
//!
//! DC-09 ("Internet Protocol Event Reporting") carries SIA DC-03 / Contact-ID
//! alarm events from an alarm panel to a central-station receiver over IP.
//! A DC-09 message is a single framed line:
//!
//! ```text
//! <LF> <CRC> <0LLL> "<token>" <seq> <Rrcvr> <Lprefix> #<account> [<data>] <CR>
//! ```
//!
//! where the CRC and length are computed over the *quoted-message* portion
//! (from the opening `"` of the token through the closing `]` of the data).
//!
//! References:
//! - ANSI/SIA DC-09-2007 "SIA DC-09 Internet Protocol Event Reporting".
//! - ANSI/SIA DC-07 Annex — the mandated CRC-16 (the CRC-16/ARC / IBM variant,
//!   reflected polynomial `0xA001`, initial value `0`).
//!
//! This module is a self-contained, pure-Rust (std-only) implementation with no
//! external dependencies. It is deterministic: identical inputs always produce
//! byte-identical output, which matters for reproducible witness bundles.

/// Compute the DC-09 CRC (CRC-16/ARC, a.k.a. CRC-16/IBM) over `data`.
///
/// Per ANSI/SIA DC-07 Annex (referenced by DC-09), the CRC is initialised to
/// `0`; for each byte the byte is XORed into the low byte of the running CRC,
/// then 8 reduction rounds are performed. Each round shifts right one bit and,
/// if the shifted-out bit was set, XORs the reflected polynomial `0xA001`.
///
/// This is the reflected-input/reflected-output CRC-16 whose check value for
/// the ASCII string `"123456789"` is `0xBB3D`.
///
/// Note: DC-09 mandates *this* CRC — the CCITT `0x1021`-style CRC is **not**
/// correct here.
pub fn crc16_dc09(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

/// Build the *quoted-message* (inner) portion of a DC-09 frame.
///
/// Layout: `"<token>"<seq:04><Rrcvr><Lprefix>#<account>[<event_body>]`.
/// The CRC and length prefixes in [`encode_dcs`] are computed over exactly this
/// string. Kept private because callers should only ever see the full frame.
fn build_inner(
    token: &str,
    seq: u16,
    receiver: &str,
    line_prefix: &str,
    account: &str,
    event_body: &str,
) -> String {
    format!(
        "\"{token}\"{seq:04}R{receiver}L{line_prefix}#{account}[{event_body}]",
        token = token,
        seq = seq % 10000,
        receiver = receiver,
        line_prefix = line_prefix,
        account = account,
        event_body = event_body,
    )
}

/// Encode a full DC-09 SIA-DCS alarm-transport message.
///
/// Produces a framed string of the form:
///
/// ```text
/// \n<CRC4><LLLL>"SIA-DCS"<seq>R<receiver>L<line_prefix>#<account>[<event_body>]\r
/// ```
///
/// * `\n` (LF, `0x0A`) opens the frame and `\r` (CR, `0x0D`) closes it.
/// * `<CRC4>` is [`crc16_dc09`] of the inner quoted-message portion, formatted
///   as 4 uppercase hex digits.
/// * `<LLLL>` is the byte length of that same portion as 4 decimal digits.
/// * `<seq>` is the message sequence number as 4 decimal digits (wraps at 9999).
/// * `event_body` is SIA event data such as `Nri1/BA001` (New event, partition
///   1, Burglary Alarm, zone 1) — build it with [`sia_event_for`].
///
/// The token is fixed to `SIA-DCS` (unencrypted SIA-DCS transport).
pub fn encode_dcs(
    seq: u16,
    receiver: &str,
    line_prefix: &str,
    account: &str,
    event_body: &str,
) -> String {
    const TOKEN: &str = "SIA-DCS";
    let inner = build_inner(TOKEN, seq, receiver, line_prefix, account, event_body);
    let crc = crc16_dc09(inner.as_bytes());
    let len = inner.len();
    // \n <CRC:04X> <LEN:04> <inner> \r
    format!("\n{crc:04X}{len:04}{inner}\r", crc = crc, len = len % 10000, inner = inner)
}

/// Map a sensing-event kind to a SIA (DC-03) event-code body for DC-09 transport.
///
/// The returned body is of the form `Nri<partition>/<CC><zzz>` where `N` marks a
/// New event, `ri1` selects partition/area 1, `CC` is the two-letter SIA event
/// code, and `zzz` is the zero-padded (3-digit) zone identifier:
///
/// | `kind`                     | SIA code | Meaning              |
/// |----------------------------|----------|----------------------|
/// | `"intrusion"` / `"presence"` | `BA`   | Burglary Alarm       |
/// | `"tamper"`                 | `TA`     | Tamper Alarm         |
/// | `"fall"`                   | `QA`     | Emergency (medical)  |
/// | *(any other)*              | `UA`     | Untyped Zone Alarm   |
///
/// The zone is always zero-padded to 3 digits (e.g. zone `1` → `001`).
pub fn sia_event_for(kind: &str, zone: u16) -> String {
    let code = match kind {
        "intrusion" | "presence" => "BA",
        "tamper" => "TA",
        "fall" => "QA",
        _ => "UA",
    };
    format!("Nri1/{code}{zone:03}", code = code, zone = zone)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Re-implement the CRC independently from the spec prose so the test does
    /// not merely echo the implementation, then assert both agree and match the
    /// canonical CRC-16/ARC check vectors.
    fn reference_crc(data: &[u8]) -> u16 {
        let mut crc: u16 = 0;
        for &b in data {
            crc ^= b as u16;
            for _ in 0..8 {
                let lsb = crc & 1;
                crc >>= 1;
                if lsb != 0 {
                    crc ^= 0xA001;
                }
            }
        }
        crc
    }

    #[test]
    fn crc16_matches_known_arc_check_vector() {
        // Canonical CRC-16/ARC check value for "123456789" is 0xBB3D.
        assert_eq!(crc16_dc09(b"123456789"), 0xBB3D);
    }

    #[test]
    fn crc16_edge_and_known_vectors() {
        assert_eq!(crc16_dc09(b""), 0x0000, "empty input -> initial value");
        assert_eq!(crc16_dc09(b"A"), 0x30C0);
        assert_eq!(crc16_dc09(b"SIA-DCS"), 0x2769);
    }

    #[test]
    fn crc16_agrees_with_independent_reference() {
        for s in [
            b"".as_slice(),
            b"123456789",
            b"SIA-DCS",
            b"\"SIA-DCS\"0001R0L0#1234[Nri1/BA001]",
            b"the quick brown fox",
        ] {
            assert_eq!(crc16_dc09(s), reference_crc(s), "mismatch on {:?}", s);
        }
    }

    #[test]
    fn crc16_is_order_sensitive() {
        assert_ne!(crc16_dc09(b"AB"), crc16_dc09(b"BA"));
    }

    #[test]
    fn encode_dcs_produces_exact_deterministic_frame() {
        let body = sia_event_for("intrusion", 1);
        let msg = encode_dcs(1, "0", "0", "1234", &body);
        // Concrete golden frame, decomposed as:
        //   \n DE96 0034 "SIA-DCS" 0001 R0 L0 #1234 [Nri1/BA001] \r
        assert_eq!(msg, "\nDE960034\"SIA-DCS\"0001R0L0#1234[Nri1/BA001]\r");
    }

    #[test]
    fn encode_dcs_frame_structure() {
        let msg = encode_dcs(42, "1", "AA", "ABCD", "Nri1/BA007");

        // Framing bytes.
        assert!(msg.starts_with('\n'), "must open with LF");
        assert!(msg.ends_with('\r'), "must close with CR");

        // Contains token, account, and event body.
        assert!(msg.contains("SIA-DCS"));
        assert!(msg.contains("#ABCD"));
        assert!(msg.contains("[Nri1/BA007]"));
        assert!(msg.contains("0042"), "seq zero-padded to 4 digits");
        assert!(msg.contains("R1L"), "receiver prefix present");
        assert!(msg.contains("LAA#"), "line prefix present");

        // Strip LF; next 4 chars are uppercase-hex CRC, following 4 are decimal len.
        let inner_frame = &msg[1..]; // drop leading LF
        let crc4 = &inner_frame[0..4];
        let len4 = &inner_frame[4..8];
        assert_eq!(crc4.len(), 4);
        assert!(
            crc4.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()),
            "CRC must be 4 uppercase hex chars, got {crc4:?}"
        );
        assert!(
            len4.chars().all(|c| c.is_ascii_digit()),
            "length must be 4 decimal digits, got {len4:?}"
        );

        // The declared length must equal the actual byte length of the quoted
        // message (everything between the length field and the trailing CR).
        let declared_len: usize = len4.parse().unwrap();
        let quoted = &inner_frame[8..inner_frame.len() - 1]; // drop trailing CR
        assert_eq!(declared_len, quoted.len());

        // And the declared CRC must match the CRC of that quoted message.
        let declared_crc = u16::from_str_radix(crc4, 16).unwrap();
        assert_eq!(declared_crc, crc16_dc09(quoted.as_bytes()));
    }

    #[test]
    fn encode_dcs_seq_zero_padding_and_wrap() {
        let m0 = encode_dcs(0, "0", "0", "AA", "Nri1/UA001");
        assert!(m0.contains("\"SIA-DCS\"0000R"));

        let m123 = encode_dcs(123, "0", "0", "AA", "Nri1/UA001");
        assert!(m123.contains("\"SIA-DCS\"0123R"));

        // seq wraps modulo 10000 to stay 4 digits.
        let mw = encode_dcs(10005, "0", "0", "AA", "Nri1/UA001");
        assert!(mw.contains("\"SIA-DCS\"0005R"), "10005 -> 0005: {mw:?}");
    }

    #[test]
    fn sia_event_for_maps_kinds() {
        assert_eq!(sia_event_for("intrusion", 1), "Nri1/BA001");
        assert_eq!(sia_event_for("presence", 12), "Nri1/BA012");
        assert_eq!(sia_event_for("tamper", 5), "Nri1/TA005");
        assert_eq!(sia_event_for("fall", 3), "Nri1/QA003");
        // Unknown kind -> untyped alarm.
        assert_eq!(sia_event_for("weather", 9), "Nri1/UA009");
        assert_eq!(sia_event_for("", 0), "Nri1/UA000");
    }

    #[test]
    fn sia_event_for_zero_pads_zone_to_three_digits() {
        assert_eq!(sia_event_for("intrusion", 7), "Nri1/BA007");
        assert_eq!(sia_event_for("intrusion", 42), "Nri1/BA042");
        assert_eq!(sia_event_for("intrusion", 100), "Nri1/BA100");
        // Zones beyond 3 digits are not truncated (spec zones are <=999, but be
        // explicit that we do not silently drop data).
        assert_eq!(sia_event_for("intrusion", 1234), "Nri1/BA1234");
    }

    #[test]
    fn end_to_end_intrusion_event() {
        // A realistic pipeline: sensing detects an intrusion in zone 3, we build
        // the SIA body and wrap it for transport to receiver "R0", line "L0",
        // account "9F1A", sequence 7.
        let body = sia_event_for("intrusion", 3);
        assert_eq!(body, "Nri1/BA003");
        let frame = encode_dcs(7, "0", "0", "9F1A", &body);
        assert!(frame.starts_with('\n') && frame.ends_with('\r'));
        assert!(frame.contains("[Nri1/BA003]"));
        assert!(frame.contains("#9F1A"));
        // Round-trip the embedded CRC/length against a fresh computation.
        let inner = &frame[9..frame.len() - 1]; // after LF+CRC4+LEN4, before CR
        assert_eq!(
            &frame[1..5],
            &format!("{:04X}", crc16_dc09(inner.as_bytes()))
        );
        assert_eq!(&frame[5..9], &format!("{:04}", inner.len()));
    }
}
