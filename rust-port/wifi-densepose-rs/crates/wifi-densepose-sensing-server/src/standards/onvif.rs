//! ONVIF event notification generation (WS-BaseNotification).
//!
//! Publishes RF presence/intrusion/tamper events in the ONVIF event format so
//! they land on a VMS timeline (Genetec, Milestone) alongside CCTV. ONVIF is an
//! open standard (SOAP / WS-Notification); no vendor SDK is required to emit
//! conformant `NotificationMessage` documents.
//!
//! Reference: ONVIF Core Spec §9 (Event Handling), OASIS WS-BaseNotification 1.3.

/// WS-BaseNotification namespace.
pub const NS_WSNT: &str = "http://docs.oasis-open.org/wsn/b-2";
/// ONVIF schema namespace (message description language).
pub const NS_TT: &str = "http://www.onvif.org/ver10/schema";
/// ONVIF topic namespace prefix.
pub const NS_TNS1: &str = "http://www.onvif.org/ver10/topics";
/// Topic dialect for a concrete (fully-qualified) topic expression.
pub const TOPIC_DIALECT_CONCRETE_SET: &str =
    "http://docs.oasis-open.org/wsn/t-1/TopicExpression/Concrete";

/// Escape the five XML predefined entities so inserted values cannot break the
/// document or inject markup.
pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Map a sensing event kind to its ONVIF topic expression.
pub fn topic_for(kind: &str) -> &'static str {
    match kind {
        "presence" | "motion" => "tns1:RuleEngine/CellMotionDetector/Motion",
        "intrusion" => "tns1:RuleEngine/FieldDetector/ObjectsInside",
        "tamper" => "tns1:Device/Trigger/tamper",
        _ => "tns1:RuleEngine/RuleEngineNotification",
    }
}

/// Build a single ONVIF `wsnt:NotificationMessage` for a boolean-state event
/// (e.g. presence true/false) originating from `source_node`.
///
/// `utc_ts_rfc3339` is the event time; `topic` should come from [`topic_for`].
pub fn presence_event_xml(
    source_node: &str,
    topic: &str,
    state: bool,
    utc_ts_rfc3339: &str,
) -> String {
    let source = escape_xml(source_node);
    let topic_esc = escape_xml(topic);
    let ts = escape_xml(utc_ts_rfc3339);
    let state_str = if state { "true" } else { "false" };
    format!(
        concat!(
            "<wsnt:NotificationMessage>",
            "<wsnt:Topic Dialect=\"{dialect}\">{topic}</wsnt:Topic>",
            "<wsnt:Message>",
            "<tt:Message UtcTime=\"{ts}\" PropertyOperation=\"Changed\">",
            "<tt:Source>",
            "<tt:SimpleItem Name=\"VideoSource\" Value=\"{source}\"/>",
            "</tt:Source>",
            "<tt:Data>",
            "<tt:SimpleItem Name=\"State\" Value=\"{state}\"/>",
            "</tt:Data>",
            "</tt:Message>",
            "</wsnt:Message>",
            "</wsnt:NotificationMessage>",
        ),
        dialect = TOPIC_DIALECT_CONCRETE_SET,
        topic = topic_esc,
        ts = ts,
        source = source,
        state = state_str,
    )
}

/// Wrap one or more `NotificationMessage` fragments into a single
/// `wsnt:Notify` envelope with the required namespace declarations.
pub fn presence_events_document(events_xml: &[String]) -> String {
    let mut body = String::new();
    for e in events_xml {
        body.push_str(e);
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <wsnt:Notify xmlns:wsnt=\"{wsnt}\" xmlns:tt=\"{tt}\" xmlns:tns1=\"{tns1}\">\
         {body}\
         </wsnt:Notify>",
        wsnt = NS_WSNT,
        tt = NS_TT,
        tns1 = NS_TNS1,
        body = body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_xml_entities() {
        assert_eq!(escape_xml("node&1"), "node&amp;1");
        assert_eq!(escape_xml("<a>\"'"), "&lt;a&gt;&quot;&apos;");
        assert_eq!(escape_xml("plain"), "plain");
    }

    #[test]
    fn topic_mapping() {
        assert_eq!(topic_for("presence"), "tns1:RuleEngine/CellMotionDetector/Motion");
        assert_eq!(topic_for("motion"), "tns1:RuleEngine/CellMotionDetector/Motion");
        assert_eq!(topic_for("intrusion"), "tns1:RuleEngine/FieldDetector/ObjectsInside");
        assert_eq!(topic_for("tamper"), "tns1:Device/Trigger/tamper");
        assert_eq!(topic_for("weird"), "tns1:RuleEngine/RuleEngineNotification");
    }

    #[test]
    fn event_xml_contains_topic_state_source_and_time() {
        let topic = topic_for("presence");
        let xml = presence_event_xml("node-1", topic, true, "2026-07-28T19:00:00Z");
        assert!(xml.contains(topic));
        assert!(xml.contains("UtcTime=\"2026-07-28T19:00:00Z\""));
        assert!(xml.contains("Name=\"State\" Value=\"true\""));
        assert!(xml.contains("Value=\"node-1\""));
        assert!(xml.contains("<wsnt:NotificationMessage>"));
    }

    #[test]
    fn event_xml_state_false() {
        let xml = presence_event_xml("node-2", topic_for("intrusion"), false, "2026-07-28T19:00:00Z");
        assert!(xml.contains("Name=\"State\" Value=\"false\""));
    }

    #[test]
    fn event_xml_escapes_source() {
        let xml = presence_event_xml("node&1", topic_for("presence"), true, "2026-07-28T19:00:00Z");
        assert!(xml.contains("Value=\"node&amp;1\""));
        assert!(!xml.contains("Value=\"node&1\""));
    }

    #[test]
    fn document_wraps_fragments_with_namespaces() {
        let e1 = presence_event_xml("node-1", topic_for("presence"), true, "2026-07-28T19:00:00Z");
        let e2 = presence_event_xml("node-2", topic_for("tamper"), true, "2026-07-28T19:00:01Z");
        let doc = presence_events_document(&[e1, e2]);
        assert!(doc.contains("xmlns:wsnt=\"http://docs.oasis-open.org/wsn/b-2\""));
        assert!(doc.contains("xmlns:tt="));
        assert!(doc.contains("<wsnt:Notify"));
        assert!(doc.contains("</wsnt:Notify>"));
        // both fragments present
        assert_eq!(doc.matches("<wsnt:NotificationMessage>").count(), 2);
        assert!(doc.contains("node-1"));
        assert!(doc.contains("node-2"));
    }
}
