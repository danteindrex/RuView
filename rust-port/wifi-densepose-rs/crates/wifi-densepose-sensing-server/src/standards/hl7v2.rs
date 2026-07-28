//! HL7 v2.x messaging adapter for the medical field.
//!
//! This module implements two independent, pure-Rust wire paths so the
//! sensing-server can plug into a hospital's admit/discharge/transfer (ADT)
//! feed and its device-vitals ingest:
//!
//! 1. **ADT ingest** (`parse_adt`) — parse an inbound HL7 v2 `ADT^A01/A02/A03`
//!    message to learn *patient identity* (MRN + name) and the *assigned bed*
//!    (patient location). This is how a bed-side WiFi sensing node is bound to a
//!    real patient without a FHIR round-trip.
//!
//! 2. **PCD-01 vitals emit** (`generate_oru_r01`) — build an IHE PCD-01
//!    conformant `ORU^R01` (unsolicited observation result) carrying the
//!    node's derived vital signs (heart rate, respiratory rate) as `OBX`
//!    segments coded with LOINC + UCUM. This is a *non-FHIR* vitals path
//!    targeting a PCD-01 "Device Observation Consumer".
//!
//! ## Standards references
//! - HL7 v2.x message framing: segments delimited by `\r` (carriage return),
//!   fields by `|`, components by `^`, repetitions by `~`, escape `\`,
//!   subcomponents by `&`. The encoding characters `^~\&` are declared in
//!   `MSH-2`.
//! - IHE PCD Technical Framework, Transaction PCD-01 ("Communicate PCD Data"):
//!   an `ORU^R01` with `MSH`, `PID`, `OBR` (Universal Service ID = vital
//!   signs), and one `OBX` per observation.
//! - LOINC codes: Heart rate `8867-4`, Respiratory rate `9279-1`.
//! - UCUM unit for a per-minute rate: `/min`.
//!
//! The functions here are deliberately dependency-free (only `std`) so the
//! module compiles standalone and can be unit-tested without any I/O.

/// Patient + bed identity extracted from an inbound HL7 v2 ADT message.
///
/// Field provenance (HL7 v2 segment.field):
/// - `mrn`         — `PID-3` (Patient Identifier List), first ID component.
/// - `family_name` — `PID-5` XPN component 1 (family name).
/// - `given_name`  — `PID-5` XPN component 2 (given name).
/// - `bed`         — `PV1-3` (Assigned Patient Location), rendered as the
///   PL components joined by `^` (e.g. `ICU^101^A`), or empty if absent.
/// - `event`       — the trigger event from `MSH-9` message type, e.g. `A01`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdtPatientBed {
    /// Medical Record Number (PID-3, first identifier).
    pub mrn: String,
    /// Patient family (last) name (PID-5 component 1).
    pub family_name: String,
    /// Patient given (first) name (PID-5 component 2).
    pub given_name: String,
    /// Assigned patient location / bed (PV1-3), `^`-joined; may be empty.
    pub bed: String,
    /// ADT trigger event code from MSH-9, e.g. `"A01"`, `"A02"`, `"A03"`.
    pub event: String,
}

/// Split a raw HL7 message into segments on `\r` and/or `\n`, trimming empties.
///
/// HL7 v2 canonically uses a bare carriage return (`\r`) as the segment
/// terminator, but real-world transports frequently rewrite line endings to
/// `\n` or `\r\n`, so we tolerate all three.
fn split_segments(raw: &str) -> Vec<&str> {
    raw.split(['\r', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Return the first segment whose 3-character segment ID equals `id`.
fn find_segment<'a>(segments: &'a [&'a str], id: &str) -> Option<&'a str> {
    segments
        .iter()
        .copied()
        .find(|seg| seg.len() >= 3 && &seg[..3] == id)
}

/// Return field `n` (1-based, HL7 convention) of a pipe-delimited segment.
///
/// Note on `MSH`: HL7 treats `MSH-1` as the field separator character itself,
/// so for an `MSH` segment the token immediately after `MSH` is `MSH-2`
/// (the encoding characters). This helper indexes the raw pipe-split tokens
/// where index 0 is the segment ID; therefore for `MSH` the caller must offset
/// by one (see `parse_adt`). For all other segments, field `n` is token `n`.
fn field(segment: &str, n: usize) -> Option<&str> {
    segment.split('|').nth(n)
}

/// Return component `c` (1-based) of a `^`-delimited field, trimmed.
fn component(field_value: &str, c: usize) -> Option<&str> {
    field_value.split('^').nth(c - 1).map(str::trim)
}

/// Parse an HL7 v2 `ADT^A01/A02/A03` message into patient + bed identity.
///
/// The parser is intentionally tolerant: it reads what it can and only returns
/// `None` when the message is not a usable ADT record — specifically when the
/// `PID` segment is missing or no MRN (`PID-3` first identifier) can be found.
/// A missing `PV1` (bed) or missing given/family name yields empty strings
/// rather than a hard failure, because ADT messages legitimately omit those.
///
/// # Trigger event
/// The event code is taken from `MSH-9` (message type), which looks like
/// `ADT^A01`; the second component (`A01`) is stored in [`AdtPatientBed::event`].
///
/// # Examples
/// ```
/// use wifi_densepose_sensing_server::standards::hl7v2::parse_adt;
/// let raw = "MSH|^~\\&|ADT|HOSP|||202607281200||ADT^A01|1|P|2.5\r\
///            PID|1||MRN12345^^^HOSP^MR||Doe^John\r\
///            PV1|1|I|ICU^101^A";
/// let p = parse_adt(raw).unwrap();
/// assert_eq!(p.mrn, "MRN12345");
/// assert_eq!(p.event, "A01");
/// ```
pub fn parse_adt(raw: &str) -> Option<AdtPatientBed> {
    let segments = split_segments(raw);
    if segments.is_empty() {
        return None;
    }

    // ---- MSH: trigger event from MSH-9 (message type, e.g. "ADT^A01") ----
    // MSH is special: MSH-1 *is* the field separator `|` itself and is not a
    // pipe-split token, so field number N maps to pipe-token index N-1:
    //   token[0]=MSH, token[1]=encoding chars (MSH-2), token[2]=MSH-3, ...
    // Therefore MSH-9 (message type) is pipe-token index 8, i.e. `field(msh, 8)`.
    let event = find_segment(&segments, "MSH")
        .and_then(|msh| field(msh, 8))
        .and_then(|msg_type| component(msg_type, 2))
        .unwrap_or("")
        .to_string();

    // ---- PID is mandatory ----
    let pid = find_segment(&segments, "PID")?;

    // PID-3: Patient Identifier List. Take the first repetition (split on '~'),
    // then its first component (the ID value) before the '^' assigning-authority
    // components.
    let mrn = field(pid, 3)
        .and_then(|f| f.split('~').next())
        .and_then(|first_rep| component(first_rep, 1))
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();

    // PID-5: Patient Name (XPN) = family^given^middle^...
    let (family_name, given_name) = match field(pid, 5) {
        Some(name) => (
            component(name, 1).unwrap_or("").to_string(),
            component(name, 2).unwrap_or("").to_string(),
        ),
        None => (String::new(), String::new()),
    };

    // ---- PV1-3: Assigned Patient Location (PL) -> "point^room^bed^facility..."
    // Render the non-empty leading components joined by '^' as the bed string.
    let bed = find_segment(&segments, "PV1")
        .and_then(|pv1| field(pv1, 3))
        .map(|loc| {
            let joined = loc
                .split('^')
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("^");
            // strip trailing empty components (e.g. "ICU^101^^^" -> "ICU^101")
            joined.trim_end_matches('^').to_string()
        })
        .unwrap_or_default();

    Some(AdtPatientBed {
        mrn,
        family_name,
        given_name,
        bed,
        event,
    })
}

/// Standard HL7 v2 encoding characters, declared in `MSH-2`.
/// Order: component `^`, repetition `~`, escape `\`, subcomponent `&`.
const HL7_ENCODING_CHARS: &str = "^~\\&";

/// A single LOINC-coded vital to render as an `OBX` segment.
struct Obx<'a> {
    /// LOINC observation identifier code, e.g. `"8867-4"`.
    loinc: &'a str,
    /// Human-readable LOINC display text, e.g. `"Heart rate"`.
    text: &'a str,
    /// Numeric observation value.
    value: f64,
}

/// Build an IHE PCD-01 conformant `ORU^R01` (unsolicited observation result)
/// carrying node-derived vital signs.
///
/// The message contains, in order:
/// - `MSH` — message header with encoding chars `^~\&`, message type `ORU^R01`,
///   HL7 version `2.6`, and the caller-supplied timestamp (`MSH-7`).
/// - `PID` — patient identity: MRN in `PID-3`, name `family^given` in `PID-5`.
/// - `OBR` — one order/report group with a Universal Service ID of
///   `Vital Signs` (PCD-01 groups all device vitals under a single `OBR`).
/// - `OBX` — one segment **per present vital**, value type `NM` (numeric),
///   LOINC-coded observation identifier, the numeric value, `/min` UCUM units,
///   and result status `F` (final).
///
/// ## Truthfulness guarantee
/// An `OBX` is emitted for a vital **only if** it is `Some` *and* strictly
/// greater than `0`. A `None` or non-positive reading is silently omitted —
/// the function never fabricates a value or emits a placeholder observation.
/// Consequently a call with all vitals absent yields a message with `MSH`,
/// `PID`, and `OBR` but zero `OBX` segments.
///
/// # Arguments
/// - `sending_app`   — populates `MSH-3` (sending application), e.g. a node id.
/// - `mrn`           — Medical Record Number for `PID-3`.
/// - `patient_family`/`patient_given` — `PID-5` XPN family/given components.
/// - `node_id`       — sensing node id, embedded in the observation sub-id /
///   `OBR-3` filler order number so results are traceable to a device.
/// - `hr_bpm`        — heart rate in beats/min (LOINC `8867-4`).
/// - `rr_bpm`        — respiratory rate in breaths/min (LOINC `9279-1`).
/// - `ts_hl7`        — an HL7 timestamp `YYYYMMDDHHMMSS` used for `MSH-7`,
///   `OBR-7` (observation date/time) and each `OBX-14`.
///
/// # Returns
/// The complete message as a single `String` with segments joined by `\r`.
pub fn generate_oru_r01(
    sending_app: &str,
    mrn: &str,
    patient_family: &str,
    patient_given: &str,
    node_id: u8,
    hr_bpm: Option<f64>,
    rr_bpm: Option<f64>,
    ts_hl7: &str,
) -> String {
    // Collect only the vitals that are present AND strictly positive.
    let mut vitals: Vec<Obx> = Vec::with_capacity(2);
    if let Some(hr) = hr_bpm {
        if hr > 0.0 {
            vitals.push(Obx {
                loinc: "8867-4",
                text: "Heart rate",
                value: hr,
            });
        }
    }
    if let Some(rr) = rr_bpm {
        if rr > 0.0 {
            vitals.push(Obx {
                loinc: "9279-1",
                text: "Respiratory rate",
                value: rr,
            });
        }
    }

    let mut segments: Vec<String> = Vec::with_capacity(3 + vitals.len());

    // ---- MSH ----
    // MSH-1 = '|' (field separator), MSH-2 = encoding chars.
    // Fields:  3 sending app | 4 sending fac | 5 recv app | 6 recv fac |
    //          7 datetime | 8 security | 9 msg type | 10 control id |
    //          11 processing id | 12 version id
    segments.push(format!(
        "MSH|{enc}|{send_app}|WIFI_DENSEPOSE|PCD_CONSUMER|HOSPITAL|{ts}||ORU^R01|{ctrl}|P|2.6",
        enc = HL7_ENCODING_CHARS,
        send_app = sending_app,
        ts = ts_hl7,
        // A deterministic message control id derived from node + timestamp so
        // the consumer can de-duplicate; not required to be globally unique
        // here but stable per (node, ts).
        ctrl = format!("{ts_hl7}{node_id}"),
    ));

    // ---- PID ----
    // PID-3 identifier list: MRN^^^assigning-authority^MR (MR = Medical Record).
    // PID-5 patient name (XPN): family^given.
    segments.push(format!(
        "PID|1||{mrn}^^^WIFI_DENSEPOSE^MR||{family}^{given}",
        mrn = mrn,
        family = patient_family,
        given = patient_given,
    ));

    // ---- OBR ----
    // Universal Service Identifier (OBR-4) = vital signs panel.
    // OBR-2 placer, OBR-3 filler = node-scoped so results trace to the device.
    segments.push(format!(
        "OBR|1|NODE{node}|NODE{node}-{ts}|{svc}|||{ts}",
        node = node_id,
        ts = ts_hl7,
        // Universal Service ID: code^text^coding-system. We use a MDC/local
        // vital-signs panel identifier as required by PCD-01's OBR grouping.
        svc = "182777000^Monitoring of patient (regime/therapy)^SCT",
    ));

    // ---- OBX (one per present vital) ----
    for (idx, v) in vitals.iter().enumerate() {
        // OBX fields:
        //  1 set id | 2 value type (NM) | 3 observation id (LOINC) |
        //  4 sub-id | 5 value | 6 units (UCUM) | 7 ref range | 8 abnormal |
        //  ... | 11 result status | ... | 14 observation datetime
        segments.push(format!(
            "OBX|{setid}|NM|{loinc}^{text}^LN|{subid}|{value}|/min^^UCUM|||||F|||{ts}",
            setid = idx + 1,
            loinc = v.loinc,
            text = v.text,
            // Sub-id ties the observation to the physical node.
            subid = format!("{node_id}.{}", idx + 1),
            value = fmt_num(v.value),
            ts = ts_hl7,
        ));
    }

    segments.join("\r")
}

/// Format a numeric HL7 `NM` value: integral values render without a trailing
/// `.0` (e.g. `72`), fractional values keep one decimal of precision
/// (e.g. `16.5`) which is the resolution clinically meaningful for these rates.
fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < f64::EPSILON {
        format!("{}", v.round() as i64)
    } else {
        format!("{:.1}", v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic ADT^A01 (admit) message with MSH/EVN/PID/PV1 segments,
    /// segment-terminated by bare `\r` as in the HL7 v2 wire format.
    const ADT_A01: &str = "MSH|^~\\&|ADT_SENDER|MEMORIAL|EHR|MEMORIAL|20260728120000||ADT^A01|MSG00001|P|2.5\rEVN|A01|20260728120000\rPID|1||MRN00987^^^MEMORIAL^MR||Smith^Jane^Q||19800101|F\rPV1|1|I|ICU^101^A^MEMORIAL||||1234^Attending^Doc";

    #[test]
    fn parse_adt_a01_extracts_identity_and_bed() {
        let p = parse_adt(ADT_A01).expect("A01 should parse");
        assert_eq!(p.mrn, "MRN00987");
        assert_eq!(p.family_name, "Smith");
        assert_eq!(p.given_name, "Jane");
        assert_eq!(p.event, "A01");
        // PV1-3 assigned location rendered as ^-joined PL components.
        assert_eq!(p.bed, "ICU^101^A^MEMORIAL");
    }

    #[test]
    fn parse_adt_tolerates_lf_line_endings() {
        let lf = ADT_A01.replace('\r', "\n");
        let p = parse_adt(&lf).expect("LF-terminated ADT should parse");
        assert_eq!(p.mrn, "MRN00987");
        assert_eq!(p.event, "A01");
    }

    #[test]
    fn parse_adt_transfer_a02_reads_new_bed() {
        let raw = "MSH|^~\\&|ADT|H|||20260728|| ADT^A02 |2|P|2.5\r\
                   PID|1||MRN55555^^^H^MR||Roe^Richard\r\
                   PV1|1|I|WARD3^305^B";
        let p = parse_adt(raw).expect("A02 should parse");
        assert_eq!(p.mrn, "MRN55555");
        assert_eq!(p.bed, "WARD3^305^B");
        // Event component has surrounding spaces trimmed by `component`.
        assert_eq!(p.event, "A02");
    }

    #[test]
    fn parse_adt_missing_pv1_yields_empty_bed_not_none() {
        let raw = "MSH|^~\\&|ADT|H|||20260728||ADT^A03|3|P|2.5\r\
                   PID|1||MRN42^^^H^MR||Nobody^Nemo";
        let p = parse_adt(raw).expect("PID present => must parse even without PV1");
        assert_eq!(p.mrn, "MRN42");
        assert_eq!(p.given_name, "Nemo");
        assert_eq!(p.bed, "");
        assert_eq!(p.event, "A03");
    }

    #[test]
    fn parse_adt_missing_name_yields_empty_strings() {
        // PID present with MRN but no PID-5 name field at all.
        let raw = "MSH|^~\\&|ADT|H|||20260728||ADT^A01|4|P|2.5\r\
                   PID|1||MRN7^^^H^MR";
        let p = parse_adt(raw).expect("MRN present => parse");
        assert_eq!(p.mrn, "MRN7");
        assert_eq!(p.family_name, "");
        assert_eq!(p.given_name, "");
    }

    #[test]
    fn parse_adt_returns_none_on_garbage() {
        assert!(parse_adt("this is not an HL7 message at all").is_none());
        assert!(parse_adt("").is_none());
        assert!(parse_adt("\r\n\r\n").is_none());
        // Has an MSH but no PID => not a usable ADT record.
        assert!(parse_adt("MSH|^~\\&|X|Y|||20260728||ADT^A01|1|P|2.5").is_none());
    }

    #[test]
    fn parse_adt_returns_none_when_mrn_absent() {
        // PID exists but PID-3 identifier field is empty.
        let raw = "MSH|^~\\&|ADT|H|||20260728||ADT^A01|1|P|2.5\r\
                   PID|1|||| Doe^Jane";
        assert!(parse_adt(raw).is_none());
    }

    #[test]
    fn generate_oru_r01_has_core_segments_and_both_vitals() {
        let msg = generate_oru_r01(
            "NODE_07",
            "MRN00987",
            "Smith",
            "Jane",
            7,
            Some(72.0),
            Some(16.0),
            "20260728120500",
        );
        let segs: Vec<&str> = msg.split('\r').collect();

        assert!(segs[0].starts_with("MSH|^~\\&|NODE_07|"));
        assert!(segs[0].contains("ORU^R01"));
        assert!(segs[0].contains("2.6")); // version id
        assert!(msg.contains("PID|1||MRN00987^^^WIFI_DENSEPOSE^MR||Smith^Jane"));
        assert!(msg.contains("OBR|1|"));

        // Exactly two OBX segments (HR + RR).
        let obx: Vec<&str> = segs.iter().copied().filter(|s| s.starts_with("OBX|")).collect();
        assert_eq!(obx.len(), 2);

        // HR OBX: LOINC 8867-4, value 72, /min UCUM units, status F, type NM.
        let hr = obx.iter().find(|s| s.contains("8867-4")).expect("HR OBX");
        assert!(hr.contains("|NM|"));
        assert!(hr.contains("8867-4^Heart rate^LN"));
        assert!(hr.contains("|72|"));
        assert!(hr.contains("/min^^UCUM"));
        assert!(hr.contains("|F|"));

        // RR OBX: LOINC 9279-1.
        let rr = obx.iter().find(|s| s.contains("9279-1")).expect("RR OBX");
        assert!(rr.contains("9279-1^Respiratory rate^LN"));
        assert!(rr.contains("|16|"));
        assert!(rr.contains("/min^^UCUM"));
    }

    #[test]
    fn generate_oru_r01_emits_only_present_vital() {
        // HR present, RR None => exactly one OBX with 8867-4 and none for RR.
        let msg = generate_oru_r01(
            "NODE_03", "MRN1", "Doe", "John", 3, Some(80.0), None, "20260728121000",
        );
        let obx: Vec<&str> = msg.split('\r').filter(|s| s.starts_with("OBX|")).collect();
        assert_eq!(obx.len(), 1);
        assert!(obx[0].contains("8867-4"));
        assert!(!msg.contains("9279-1"));
        // Set id of the sole OBX is 1.
        assert!(obx[0].starts_with("OBX|1|NM|"));
    }

    #[test]
    fn generate_oru_r01_never_fabricates_nonpositive_or_absent() {
        // Zero and negative are treated as "no reading" and omitted.
        let msg = generate_oru_r01(
            "N", "MRN1", "Doe", "John", 1, Some(0.0), Some(-5.0), "20260728121500",
        );
        let obx_count = msg.split('\r').filter(|s| s.starts_with("OBX|")).count();
        assert_eq!(obx_count, 0);
        // Still a well-formed message with MSH/PID/OBR.
        assert!(msg.contains("MSH|"));
        assert!(msg.contains("PID|"));
        assert!(msg.contains("OBR|"));
    }

    #[test]
    fn generate_oru_r01_formats_fractional_rate() {
        // Respiratory rate 16.5 should render with one decimal, not "16.5000..".
        let msg = generate_oru_r01(
            "N", "MRN1", "Doe", "John", 1, None, Some(16.5), "20260728122000",
        );
        assert!(msg.contains("|16.5|"), "expected fractional RR value, got: {msg}");
        assert!(msg.contains("9279-1"));
    }

    #[test]
    fn generate_oru_r01_segment_terminator_is_cr() {
        let msg = generate_oru_r01(
            "N", "MRN1", "Doe", "John", 1, Some(60.0), None, "20260728122500",
        );
        assert!(msg.contains('\r'));
        assert!(!msg.contains('\n'));
    }

    #[test]
    fn roundtrip_adt_identity_into_oru() {
        // Parse identity from ADT then emit an ORU for that patient: the MRN
        // and name must survive the hand-off.
        let p = parse_adt(ADT_A01).unwrap();
        let msg = generate_oru_r01(
            "NODE_07",
            &p.mrn,
            &p.family_name,
            &p.given_name,
            7,
            Some(72.0),
            Some(15.0),
            "20260728120500",
        );
        assert!(msg.contains("MRN00987"));
        assert!(msg.contains("Smith^Jane"));
    }
}
