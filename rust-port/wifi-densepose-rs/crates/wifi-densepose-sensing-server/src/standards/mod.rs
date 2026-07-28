//! Interoperability standards adapters for the medical + physical-security
//! fields. Each submodule is a self-contained, pure-Rust implementation of a
//! wire standard so the sensing-server can plug into hospital EHR/alarm and
//! security-operations infrastructure.
//!
//! | Module   | Standard | Field | Purpose |
//! |----------|----------|-------|---------|
//! | `alarm`  | IEC 60601-1-8 | medical | Clinical alarm state machine (priority, latch, ack, silence, escalate, audit) |
//! | `hl7v2`  | HL7 v2.x / IHE PCD-01 | medical | ADT patient/bed ingest + ORU^R01 vitals emit |
//! | `dc09`   | ANSI/SIA DC-09 | security | Alarm transport to a central-station receiver |
//! | `onvif`  | ONVIF (WS-BaseNotification) | security | Event push onto a VMS timeline |

pub mod alarm;
pub mod dc09;
pub mod hl7v2;
pub mod onvif;
