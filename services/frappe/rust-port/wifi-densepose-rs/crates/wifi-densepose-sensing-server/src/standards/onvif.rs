//! ONVIF WS-BaseNotification event message generation.
//!
//! Produces ONVIF-flavoured event notifications so that RF presence /
//! intrusion / tamper events detected by the sensing server can appear on a
//! Video Management System (VMS) timeline (e.g. Genetec Security Center,
//! Milestone XProtect) alongside camera analytics.
//!
//! Standards references:
//! - ONVIF Core Specification, section 9 "Event handling".
//! - OASIS WS-BaseNotification 1.3 (`wsnt` namespace
//!   `http://docs.oasis-open.org/wsn/b-2`) — the `<Notify>` /
//!   `<NotificationMessage>` / `<Topic>` / `<Message>` envelope.
//! - ONVIF schema `tt` namespace `http://www.onvif.org/ver10/schema` — the
//!   `<tt:Message>`, `<tt:Source>` and `<tt:Data>` `SimpleItem` payload.
//!
//! Topics follow the ONVIF topic-namespace convention (`tns1:...`) and the
//! `<Topic>` element declares the concrete-set dialect defined by
//! WS-Topics (`.../TopicExpression/ConcreteSet`).
//!
//! The functions here are pure and allocation-only — no async, no I/O — so
//! they can be unit tested in isolation and composed by the transport layer.

/// WS-BaseNotification base namespace (`wsnt`).
pub const NS_WSNT: &str = "http://docs.oasis-open.org/wsn/b-2";

/// ONVIF schema namespace (`tt`).
pub const NS_TT: &str = "http://www.onvif.org/ver10/schema";

/// ONVIF topic-namespace namespace (`tns1`).
pub const NS_TNS1: &str = "http://www.onvif.org/ver10/topics";

/// WS-Topics concrete-set topic-expression dialect used by the `<Topic>`
/// `Dialect` attribute.
pub const TOPIC_DIALECT_CONCRETE_SET: &str =
    "http://docs.oasis-open.org/wsn/t-1/TopicExpression/ConcreteSet";

/// Escape the five XML predefined entities (`&`, `<`, `>`, `"`, `'`) so an
/// arbitrary string can be safely inserted into element text or a
/// double-quoted attribute value.
///
/// `&` is replaced first so already-escaped output is not double-escaped by a
/// later rule.
pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// Map a semantic event `kind` to its ONVIF topic path (`tns1:...`).
///
/// - `"presence"` / `"motion"` → cell-motion detector `Motion` topic.
/// - `"intrusion"`             → field detector `ObjectsInside` topic.
/// - `"tamper"`                → device trigger `tamper` topic.
/// - anything else             → generic rule-engine notification topic.
pub fn topic_for(kind: &str) -> &'static str {
    match kind {
        "presence" | "motion" => "tns1:RuleEngine/CellMotionDetector/Motion",
        "intrusion" => "tns1:RuleEngine/FieldDetector/ObjectsInside",
        "tamper" => "tns1:Device/Trigger/tamper",
        _ => "tns1:RuleEngine/RuleEngineNotification",
    }
}

/// Build a single ONVIF WS-BaseNotification `<wsnt:NotificationMessage>`
/// fragment describing a boolean state change on `source_node`.
///
/// The fragment carries:
/// - a `<wsnt:Topic>` with the concrete-set dialect and the supplied `topic`
///   path (typically obtained from [`topic_for`]);
/// - a `<tt:Message>` stamped with `UtcTime` (an RFC 3339 / ISO 8601
///   timestamp — the caller is responsible for providing a valid one);
/// - a `<tt:Source>` `SimpleItem` `Name="VideoSource" Value=source_node`;
/// - a `<tt:Data>` `SimpleItem` `Name="State"` whose value is the string
///   `"true"` or `"false"` derived from `state`.
///
/// All interpolated values are XML-escaped via [`escape_xml`]. The returned
/// fragment is not a standalone document — wrap one or more fragments with
/// [`presence_events_document`] to obtain a namespace-declaring envelope.
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
        "<wsnt:NotificationMessage>\
<wsnt:Topic Dialect=\"{dialect}\">{topic}</wsnt:Topic>\
<wsnt:Message>\
<tt:Message UtcTime=\"{ts}\" PropertyOperation=\"Changed\">\
<tt:Source>\
<tt:SimpleItem Name=\"VideoSource\" Value=\"{source}\"/>\
</tt:Source>\
<tt:Data>\
<tt:SimpleItem Name=\"State\" Value=\"{state}\"/>\
</tt:Data>\
</tt:Message>\
</wsnt:Message>\
</wsnt:NotificationMessage>",
        dialect = escape_xml(TOPIC_DIALECT_CONCRETE_SET),
        topic = topic_esc,
        ts = ts,
        source = source,
        state = state_str,
    )
}

/// Wrap one or more [`presence_event_xml`] fragments in a single
/// `<wsnt:Notify>` envelope that declares the `wsnt`, `tt` and `tns1`
/// namespaces.
///
/// This produces the top-level document a VMS event sink expects when
/// polling / receiving pushed ONVIF notifications.
pub fn presence_events_document(events_xml: &[String]) -> String {
    let mut body = String::new();
    for fragment in events_xml {
        body.push_str(fragment);
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<wsnt:Notify xmlns:wsnt=\"{ns_wsnt}\" xmlns:tt=\"{ns_tt}\" xmlns:tns1=\"{ns_tns1}\">\
{body}\
</wsnt:Notify>",
        ns_wsnt = NS_WSNT,
        ns_tt = NS_TT,
        ns_tns1 = NS_TNS1,
        body = body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_xml_handles_all_predefined_entities() {
        assert_eq!(escape_xml("a&b"), "a&amp;b");
        assert_eq!(escape_xml("a<b>c"), "a&lt;b&gt;c");
        assert_eq!(escape_xml("say \"hi\""), "say &quot;hi&quot;");
        assert_eq!(escape_xml("it's"), "it&apos;s");
        // Ampersand is escaped exactly once (no double-escaping).
        assert_eq!(escape_xml("&lt;"), "&amp;lt;");
        // Plain text passes through unchanged.
        assert_eq!(escape_xml("node-1_abc"), "node-1_abc");
    }

    #[test]
    fn presence_event_contains_topic_timestamp_state_and_source() {
        let topic = topic_for("presence");
        let xml = presence_event_xml("node-7", topic, true, "2026-07-26T12:34:56Z");

        // Topic path is present.
        assert!(
            xml.contains("tns1:RuleEngine/CellMotionDetector/Motion"),
            "topic missing: {xml}"
        );
        // Concrete-set dialect is declared on the Topic element.
        assert!(
            xml.contains(&format!("Dialect=\"{}\"", escape_xml(TOPIC_DIALECT_CONCRETE_SET))),
            "dialect missing: {xml}"
        );
        // UtcTime attribute carries the supplied timestamp.
        assert!(
            xml.contains("UtcTime=\"2026-07-26T12:34:56Z\""),
            "UtcTime missing: {xml}"
        );
        // State value matches the bool.
        assert!(
            xml.contains("Name=\"State\" Value=\"true\""),
            "state=true missing: {xml}"
        );
        // Source node appears as the VideoSource SimpleItem.
        assert!(
            xml.contains("Name=\"VideoSource\" Value=\"node-7\""),
            "source missing: {xml}"
        );
        // Structural envelope elements exist.
        assert!(xml.contains("<wsnt:NotificationMessage>"));
        assert!(xml.contains("</wsnt:NotificationMessage>"));
        assert!(xml.contains("<tt:Message"));
        assert!(xml.contains("<tt:Source>"));
        assert!(xml.contains("<tt:Data>"));
    }

    #[test]
    fn presence_event_state_false_renders_false() {
        let xml = presence_event_xml("cam", topic_for("motion"), false, "2026-01-01T00:00:00Z");
        assert!(
            xml.contains("Name=\"State\" Value=\"false\""),
            "state=false missing: {xml}"
        );
        assert!(!xml.contains("Value=\"true\""), "unexpected true: {xml}");
    }

    #[test]
    fn presence_event_escapes_special_chars_in_source() {
        let xml = presence_event_xml("node&1", topic_for("presence"), true, "2026-07-26T00:00:00Z");
        // Raw ampersand must be escaped.
        assert!(
            xml.contains("Value=\"node&amp;1\""),
            "source not escaped: {xml}"
        );
        // The unescaped form must not survive in the output.
        assert!(
            !xml.contains("node&1\""),
            "raw ampersand leaked: {xml}"
        );
    }

    #[test]
    fn presence_event_escapes_quotes_and_angle_brackets_in_source() {
        let xml =
            presence_event_xml("a\"<b>", topic_for("presence"), true, "2026-07-26T00:00:00Z");
        assert!(xml.contains("a&quot;&lt;b&gt;"), "escaping wrong: {xml}");
        // No raw angle bracket from the injected value should appear as an
        // opening tag start immediately followed by 'b>'.
        assert!(!xml.contains("<b>"), "raw markup leaked: {xml}");
    }

    #[test]
    fn topic_for_maps_known_kinds() {
        assert_eq!(
            topic_for("presence"),
            "tns1:RuleEngine/CellMotionDetector/Motion"
        );
        assert_eq!(
            topic_for("motion"),
            "tns1:RuleEngine/CellMotionDetector/Motion"
        );
        assert_eq!(
            topic_for("intrusion"),
            "tns1:RuleEngine/FieldDetector/ObjectsInside"
        );
        assert_eq!(topic_for("tamper"), "tns1:Device/Trigger/tamper");
    }

    #[test]
    fn topic_for_unknown_kind_falls_back_to_generic() {
        assert_eq!(
            topic_for("something-else"),
            "tns1:RuleEngine/RuleEngineNotification"
        );
        assert_eq!(
            topic_for(""),
            "tns1:RuleEngine/RuleEngineNotification"
        );
    }

    #[test]
    fn document_wraps_multiple_fragments_and_declares_namespaces() {
        let e1 = presence_event_xml("node-a", topic_for("presence"), true, "2026-07-26T00:00:00Z");
        let e2 = presence_event_xml("node-b", topic_for("intrusion"), false, "2026-07-26T00:00:01Z");
        let doc = presence_events_document(&[e1.clone(), e2.clone()]);

        // Envelope element with wsnt namespace declaration.
        assert!(doc.contains("<wsnt:Notify"), "no Notify element: {doc}");
        assert!(
            doc.contains(&format!("xmlns:wsnt=\"{}\"", NS_WSNT)),
            "wsnt namespace missing: {doc}"
        );
        assert!(
            doc.contains(&format!("xmlns:tt=\"{}\"", NS_TT)),
            "tt namespace missing: {doc}"
        );
        assert!(
            doc.contains(&format!("xmlns:tns1=\"{}\"", NS_TNS1)),
            "tns1 namespace missing: {doc}"
        );
        assert!(doc.contains("</wsnt:Notify>"), "no closing Notify: {doc}");

        // Both fragments are embedded verbatim.
        assert!(doc.contains(&e1), "fragment 1 missing");
        assert!(doc.contains(&e2), "fragment 2 missing");

        // Exactly two NotificationMessage elements were wrapped.
        assert_eq!(
            doc.matches("<wsnt:NotificationMessage>").count(),
            2,
            "expected 2 notification messages: {doc}"
        );

        // XML prolog is present exactly once.
        assert_eq!(doc.matches("<?xml").count(), 1);
    }

    #[test]
    fn empty_document_still_declares_namespaces() {
        let doc = presence_events_document(&[]);
        assert!(doc.contains("<wsnt:Notify"));
        assert!(doc.contains(&format!("xmlns:wsnt=\"{}\"", NS_WSNT)));
        assert!(doc.contains("</wsnt:Notify>"));
        assert_eq!(doc.matches("<wsnt:NotificationMessage>").count(), 0);
    }

    #[test]
    fn balanced_open_close_tags_in_fragment() {
        let xml = presence_event_xml("n", topic_for("tamper"), true, "2026-07-26T00:00:00Z");
        // Each container tag opens and closes once (well-formed-ish check).
        for tag in [
            "wsnt:NotificationMessage",
            "wsnt:Topic",
            "wsnt:Message",
            "tt:Message",
            "tt:Source",
            "tt:Data",
        ] {
            assert_eq!(
                xml.matches(&format!("<{tag}")).count(),
                1,
                "open count wrong for {tag}: {xml}"
            );
        }
        assert_eq!(xml.matches("</wsnt:NotificationMessage>").count(), 1);
        assert_eq!(xml.matches("</tt:Message>").count(), 1);
    }
}
