//! IEC 60601-1-8 clinical alarm-system state machine.
//!
//! IEC 60601-1-8 is the collateral standard for alarm systems in medical
//! electrical equipment. This module implements the parts of that standard that
//! govern the *behaviour* of an alarm signal, independent of the audible/visual
//! rendering:
//!
//! - **Alarm priority** (Clause 3, "alarm priority"): every alarm condition is
//!   assigned a **HIGH**, **MEDIUM**, or **LOW** priority reflecting the onset
//!   time and severity of the underlying hazard. HIGH ⇒ immediate operator
//!   response required; MEDIUM ⇒ prompt response; LOW ⇒ awareness.
//! - **Alarm state**: an alarm condition transitions through an *active*,
//!   *acknowledged*, *silenced*, and *cleared* lifecycle. IEC 60601-1-8 uses the
//!   term "alarm signal inactivation state" for the acknowledged/silenced
//!   (audio-paused) conditions; we model them as explicit states.
//! - **Latching** (Clause 6.11.2.2, "latching alarm signals"): a HIGH-priority
//!   alarm signal is *latched* — once activated it remains active until the
//!   operator explicitly resets (clears) it, even if the physiological
//!   condition that triggered it has since resolved. This prevents transient
//!   life-threatening events (e.g. a momentary apnea) from being missed.
//! - **Escalation** (Clause 6.11.2.1, "alarm signal generation" + the general
//!   requirement that an unattended alarm not be silently dropped): an alarm
//!   that is not acknowledged within an operator-response window is *escalated*
//!   so it can be re-annunciated or forwarded to a supervisor / distributed
//!   alarm system.
//! - **Audit trail**: IEC 60601-1-8 (and IEC 62443 / risk-management practice
//!   for connected devices) requires that alarm-condition and operator actions
//!   be logged. Every state transition here appends an immutable audit entry.
//!
//! The module is deliberately dependency-light: `std`, `serde` (derive),
//! `serde_json` (via the endpoint layer), and `chrono` only. Timestamps are
//! passed in as RFC 3339 strings by the caller so the state machine itself is
//! pure and deterministic (no wall-clock reads), which keeps it testable and
//! reproducible for witness bundles. `chrono` is used only to *parse* the
//! caller-supplied timestamps for escalation age comparisons.

use std::collections::HashMap;

use chrono::DateTime;
use serde::Serialize;

/// Alarm priority per IEC 60601-1-8 Clause 3.
///
/// Priority encodes the urgency of the operator response required for the
/// underlying alarm condition. It is a fixed property of an alarm kind and does
/// not change over the alarm's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AlarmPriority {
    /// Immediate operator response required (life-threatening / high-onset).
    High,
    /// Prompt operator response required.
    Medium,
    /// Operator awareness required.
    Low,
}

impl AlarmPriority {
    /// Numeric, CEF-like severity for this priority.
    ///
    /// The value follows the ArcSight Common Event Format (CEF) severity scale
    /// (0–10, higher = more severe), mapping the three IEC priorities onto
    /// well-separated bands so downstream SIEM / CEF consumers can threshold on
    /// them: HIGH → 9, MEDIUM → 6, LOW → 3. This is a *reporting* projection,
    /// not a clinical ordering beyond `High > Medium > Low`.
    pub fn severity(self) -> u8 {
        match self {
            AlarmPriority::High => 9,
            AlarmPriority::Medium => 6,
            AlarmPriority::Low => 3,
        }
    }
}

/// Lifecycle state of an individual alarm.
///
/// - `Active`: the alarm condition is annunciating and demands attention.
/// - `Acknowledged`: an operator has confirmed awareness; audio is paused but
///   the condition is still present (IEC 60601-1-8 "alarm signal inactivation").
/// - `Silenced`: audio is paused with an operator-supplied reason (audio-paused
///   / audio-off); the condition is still present.
/// - `Cleared`: the alarm has been reset by the operator (or auto-cleared for
///   non-latching alarms) and is no longer active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AlarmState {
    Active,
    Acknowledged,
    Silenced,
    Cleared,
}

/// A single clinical alarm instance.
#[derive(Debug, Clone, Serialize)]
pub struct Alarm {
    /// Monotonic, manager-assigned identifier (unique within an `AlarmManager`).
    pub id: u64,
    /// Origin of the alarm condition, e.g. `"node-1"`.
    pub source: String,
    /// Alarm-condition kind, e.g. `"fall"`, `"no-breathing"`, `"hr-critical"`.
    pub kind: String,
    /// IEC 60601-1-8 priority of this alarm.
    pub priority: AlarmPriority,
    /// Current lifecycle state.
    pub state: AlarmState,
    /// RFC 3339 timestamp at which the alarm was first raised (caller-supplied).
    pub raised_ts: String,
    /// RFC 3339 timestamp of the most recent state transition.
    pub updated_ts: String,
    /// Operator who acknowledged the alarm, if any.
    pub ack_by: Option<String>,
    /// Reason recorded when the alarm was silenced, if any.
    pub silence_reason: Option<String>,
    /// Whether this alarm has been escalated for lack of timely acknowledgement.
    pub escalated: bool,
}

/// Kinds of auditable action recorded against an alarm.
///
/// String-valued in the entry so the audit log is trivially serialisable and
/// human-readable; the constants below are the canonical action names.
pub mod audit_action {
    pub const RAISED: &str = "raised";
    pub const DEDUPED: &str = "deduped";
    pub const ACKNOWLEDGED: &str = "acknowledged";
    pub const SILENCED: &str = "silenced";
    pub const CLEARED: &str = "cleared";
    pub const ESCALATED: &str = "escalated";
}

/// An append-only audit record for one alarm action.
///
/// The audit log is never mutated in place, satisfying the IEC 60601-1-8 /
/// connected-device requirement for a tamper-evident record of alarm-system
/// activity and operator responses.
#[derive(Debug, Clone, Serialize)]
pub struct AlarmAuditEntry {
    /// Id of the alarm this entry concerns.
    pub alarm_id: u64,
    /// Action taken (see [`audit_action`]).
    pub action: String,
    /// Operator responsible, when the action was operator-initiated.
    pub actor: Option<String>,
    /// Free-text reason, when supplied (e.g. silence reason).
    pub reason: Option<String>,
    /// RFC 3339 timestamp of the action (caller-supplied).
    pub ts: String,
}

/// In-memory manager for the active alarm set plus its immutable audit trail.
///
/// One `AlarmManager` typically owns all alarms for a deployment. It is `Send`
/// (all fields are owned/`Send`) but is not internally synchronised; the caller
/// wraps it in a `Mutex`/`RwLock` if shared across threads.
#[derive(Debug, Default)]
pub struct AlarmManager {
    active: HashMap<u64, Alarm>,
    next_id: u64,
    audit_log: Vec<AlarmAuditEntry>,
}

impl AlarmManager {
    /// Create an empty manager. The first raised alarm receives id `1`.
    pub fn new() -> Self {
        AlarmManager {
            active: HashMap::new(),
            next_id: 0,
            audit_log: Vec::new(),
        }
    }

    /// Append an audit entry (the only path that mutates the audit log).
    fn log(
        &mut self,
        alarm_id: u64,
        action: &str,
        actor: Option<String>,
        reason: Option<String>,
        ts: &str,
    ) {
        self.audit_log.push(AlarmAuditEntry {
            alarm_id,
            action: action.to_string(),
            actor,
            reason,
            ts: ts.to_string(),
        });
    }

    /// Raise an alarm condition, returning its id.
    ///
    /// **Deduplication**: IEC 60601-1-8 discourages redundant re-annunciation of
    /// an already-present alarm condition. If an alarm with the same `source`
    /// and `kind` is already `Active`, `Acknowledged`, or `Silenced` (i.e. not
    /// cleared), no new alarm is created — the existing alarm's id is returned
    /// and a `deduped` audit entry is recorded. A previously `Cleared` alarm
    /// does not block a fresh raise (it is retained only for audit history; see
    /// [`clear`](Self::clear)).
    pub fn raise(
        &mut self,
        source: &str,
        kind: &str,
        priority: AlarmPriority,
        ts: &str,
    ) -> u64 {
        if let Some(existing) = self.active.values().find(|a| {
            a.source == source && a.kind == kind && a.state != AlarmState::Cleared
        }) {
            let id = existing.id;
            self.log(id, audit_action::DEDUPED, None, None, ts);
            return id;
        }

        self.next_id += 1;
        let id = self.next_id;
        let alarm = Alarm {
            id,
            source: source.to_string(),
            kind: kind.to_string(),
            priority,
            state: AlarmState::Active,
            raised_ts: ts.to_string(),
            updated_ts: ts.to_string(),
            ack_by: None,
            silence_reason: None,
            escalated: false,
        };
        self.active.insert(id, alarm);
        self.log(id, audit_action::RAISED, None, None, ts);
        id
    }

    /// Acknowledge an alarm: an operator confirms awareness.
    ///
    /// Transitions the alarm to [`AlarmState::Acknowledged`], records the actor,
    /// and appends an audit entry. Returns `false` (and logs nothing) if the id
    /// is unknown or the alarm is already `Cleared`.
    pub fn acknowledge(&mut self, id: u64, actor: &str, ts: &str) -> bool {
        match self.active.get_mut(&id) {
            Some(alarm) if alarm.state != AlarmState::Cleared => {
                alarm.state = AlarmState::Acknowledged;
                alarm.ack_by = Some(actor.to_string());
                alarm.updated_ts = ts.to_string();
                self.log(id, audit_action::ACKNOWLEDGED, Some(actor.to_string()), None, ts);
                true
            }
            _ => false,
        }
    }

    /// Silence an alarm with a mandatory reason (IEC 60601-1-8 audio-paused /
    /// audio-off state).
    ///
    /// Transitions to [`AlarmState::Silenced`], stores the `reason` on the alarm
    /// and in the audit entry. Returns `false` if the id is unknown or the alarm
    /// is already `Cleared`.
    pub fn silence(&mut self, id: u64, reason: &str, ts: &str) -> bool {
        match self.active.get_mut(&id) {
            Some(alarm) if alarm.state != AlarmState::Cleared => {
                alarm.state = AlarmState::Silenced;
                alarm.silence_reason = Some(reason.to_string());
                alarm.updated_ts = ts.to_string();
                self.log(id, audit_action::SILENCED, None, Some(reason.to_string()), ts);
                true
            }
            _ => false,
        }
    }

    /// Clear (reset) an alarm.
    ///
    /// This is the operator-driven reset that a *latched* HIGH-priority alarm
    /// requires (Clause 6.11.2.2): only an explicit `clear` returns a latched
    /// alarm to the `Cleared` state. The alarm is marked `Cleared` and removed
    /// from the active set (its history remains in the audit log). Returns
    /// `false` if the id is unknown or the alarm was already cleared.
    pub fn clear(&mut self, id: u64, ts: &str) -> bool {
        match self.active.get_mut(&id) {
            Some(alarm) if alarm.state != AlarmState::Cleared => {
                alarm.state = AlarmState::Cleared;
                alarm.updated_ts = ts.to_string();
                self.log(id, audit_action::CLEARED, None, None, ts);
                // Remove from the active set; the audit trail preserves history.
                self.active.remove(&id);
                true
            }
            _ => false,
        }
    }

    /// Escalate un-acknowledged alarms whose age exceeds their priority window.
    ///
    /// An alarm is a candidate for escalation while it is still `Active` (an
    /// `Acknowledged` or `Silenced` alarm has already had an operator response,
    /// so it is *not* escalated). For each still-`Active`, not-yet-`escalated`
    /// alarm:
    ///
    /// - HIGH-priority alarms older than `high_secs` seconds are escalated;
    /// - MEDIUM-priority alarms older than `medium_secs` seconds are escalated;
    /// - LOW-priority alarms are not escalated by this policy.
    ///
    /// Age is `now_ts − raised_ts`, both parsed from RFC 3339. Alarms with an
    /// unparseable `raised_ts` are skipped. Returns the ids that were newly
    /// escalated (each also gets an `escalated` audit entry). Idempotent: an
    /// already-escalated alarm is not re-escalated.
    pub fn escalate_due(&mut self, now_ts: &str, high_secs: i64, medium_secs: i64) -> Vec<u64> {
        let now = match DateTime::parse_from_rfc3339(now_ts) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let mut escalated_ids = Vec::new();
        for alarm in self.active.values_mut() {
            if alarm.escalated || alarm.state != AlarmState::Active {
                continue;
            }
            let raised = match DateTime::parse_from_rfc3339(&alarm.raised_ts) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let age = (now - raised).num_seconds();
            let threshold = match alarm.priority {
                AlarmPriority::High => Some(high_secs),
                AlarmPriority::Medium => Some(medium_secs),
                AlarmPriority::Low => None,
            };
            if let Some(limit) = threshold {
                if age > limit {
                    alarm.escalated = true;
                    alarm.updated_ts = now_ts.to_string();
                    escalated_ids.push(alarm.id);
                }
            }
        }

        // Deterministic order + audit logging after the mutable borrow ends.
        escalated_ids.sort_unstable();
        for &id in &escalated_ids {
            self.log(id, audit_action::ESCALATED, None, None, now_ts);
        }
        escalated_ids
    }

    /// All currently active (non-cleared) alarms, in ascending id order.
    pub fn active(&self) -> Vec<&Alarm> {
        let mut alarms: Vec<&Alarm> = self.active.values().collect();
        alarms.sort_by_key(|a| a.id);
        alarms
    }

    /// The immutable, append-only audit trail.
    pub fn audit(&self) -> &[AlarmAuditEntry] {
        &self.audit_log
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixed RFC 3339 timestamps used across tests.
    const T0: &str = "2026-07-28T10:00:00Z";
    const T1: &str = "2026-07-28T10:00:05Z";
    const T2: &str = "2026-07-28T10:00:20Z";

    #[test]
    fn priority_severity_is_cef_like_and_ordered() {
        assert_eq!(AlarmPriority::High.severity(), 9);
        assert_eq!(AlarmPriority::Medium.severity(), 6);
        assert_eq!(AlarmPriority::Low.severity(), 3);
        assert!(AlarmPriority::High.severity() > AlarmPriority::Medium.severity());
        assert!(AlarmPriority::Medium.severity() > AlarmPriority::Low.severity());
    }

    #[test]
    fn raise_assigns_monotonic_ids_and_active_state() {
        let mut m = AlarmManager::new();
        let a = m.raise("node-1", "fall", AlarmPriority::High, T0);
        let b = m.raise("node-2", "hr-critical", AlarmPriority::High, T0);
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(m.active().len(), 2);
        let alarm = m.active().into_iter().find(|x| x.id == a).unwrap();
        assert_eq!(alarm.state, AlarmState::Active);
        assert_eq!(alarm.source, "node-1");
        assert_eq!(alarm.kind, "fall");
        assert!(!alarm.escalated);
    }

    #[test]
    fn raise_dedupes_same_source_and_kind() {
        let mut m = AlarmManager::new();
        let first = m.raise("node-1", "no-breathing", AlarmPriority::High, T0);
        let dup = m.raise("node-1", "no-breathing", AlarmPriority::High, T1);
        // Same id returned, no duplicate alarm created.
        assert_eq!(first, dup);
        assert_eq!(m.active().len(), 1);
        // A different kind on the same node is NOT a duplicate.
        let other = m.raise("node-1", "fall", AlarmPriority::Medium, T1);
        assert_ne!(other, first);
        assert_eq!(m.active().len(), 2);
        // Audit records the raise, the dedupe, and the second raise.
        let actions: Vec<&str> = m.audit().iter().map(|e| e.action.as_str()).collect();
        assert_eq!(actions, vec![
            audit_action::RAISED,
            audit_action::DEDUPED,
            audit_action::RAISED,
        ]);
    }

    #[test]
    fn acknowledge_transitions_state_records_actor_and_audits() {
        let mut m = AlarmManager::new();
        let id = m.raise("node-1", "fall", AlarmPriority::High, T0);
        let audit_before = m.audit().len();

        let ok = m.acknowledge(id, "nurse-jane", T1);
        assert!(ok);

        let alarm = m.active().into_iter().find(|a| a.id == id).unwrap();
        assert_eq!(alarm.state, AlarmState::Acknowledged);
        assert_eq!(alarm.ack_by.as_deref(), Some("nurse-jane"));
        assert_eq!(alarm.updated_ts, T1);

        // Exactly one new audit entry, correctly attributed.
        assert_eq!(m.audit().len(), audit_before + 1);
        let entry = m.audit().last().unwrap();
        assert_eq!(entry.action, audit_action::ACKNOWLEDGED);
        assert_eq!(entry.alarm_id, id);
        assert_eq!(entry.actor.as_deref(), Some("nurse-jane"));
        assert_eq!(entry.ts, T1);

        // Acknowledging an unknown id fails and does not grow the log.
        let n = m.audit().len();
        assert!(!m.acknowledge(9999, "nurse-jane", T1));
        assert_eq!(m.audit().len(), n);
    }

    #[test]
    fn silence_records_reason_on_alarm_and_in_audit() {
        let mut m = AlarmManager::new();
        let id = m.raise("node-3", "hr-critical", AlarmPriority::Medium, T0);

        let ok = m.silence(id, "clinician at bedside", T1);
        assert!(ok);

        let alarm = m.active().into_iter().find(|a| a.id == id).unwrap();
        assert_eq!(alarm.state, AlarmState::Silenced);
        assert_eq!(alarm.silence_reason.as_deref(), Some("clinician at bedside"));

        let entry = m.audit().last().unwrap();
        assert_eq!(entry.action, audit_action::SILENCED);
        assert_eq!(entry.reason.as_deref(), Some("clinician at bedside"));
    }

    #[test]
    fn escalate_due_marks_old_unacked_high_alarms() {
        let mut m = AlarmManager::new();
        // Raised at T0 (10:00:00).
        let high = m.raise("node-1", "no-breathing", AlarmPriority::High, T0);
        let medium = m.raise("node-2", "hr-critical", AlarmPriority::Medium, T0);
        let low = m.raise("node-3", "presence-lost", AlarmPriority::Low, T0);
        // Acknowledged high alarm should NOT escalate.
        let acked = m.raise("node-4", "fall", AlarmPriority::High, T0);
        assert!(m.acknowledge(acked, "nurse", T0));

        // now = T2 (10:00:20), 20s later. high_secs=10, medium_secs=30.
        let escalated = m.escalate_due(T2, 10, 30);

        // Only the un-acked HIGH alarm crosses its 10s window; MEDIUM (30s) and
        // LOW (never) do not, and the acked HIGH is excluded.
        assert_eq!(escalated, vec![high]);
        assert!(m.active().into_iter().find(|a| a.id == high).unwrap().escalated);
        assert!(!m.active().into_iter().find(|a| a.id == medium).unwrap().escalated);
        assert!(!m.active().into_iter().find(|a| a.id == low).unwrap().escalated);
        assert!(!m.active().into_iter().find(|a| a.id == acked).unwrap().escalated);

        // An escalated audit entry was appended for the high alarm.
        assert!(m.audit().iter().any(|e| e.alarm_id == high && e.action == audit_action::ESCALATED));

        // Idempotent: a second call at the same time re-escalates nothing.
        assert!(m.escalate_due(T2, 10, 30).is_empty());
    }

    #[test]
    fn escalate_due_eventually_catches_medium() {
        let mut m = AlarmManager::new();
        let medium = m.raise("node-2", "hr-critical", AlarmPriority::Medium, T0);
        // 20s elapsed but medium window is 30s → not yet.
        assert!(m.escalate_due(T2, 10, 30).is_empty());
        // 40s elapsed → medium now escalates.
        let later = "2026-07-28T10:00:40Z";
        assert_eq!(m.escalate_due(later, 10, 30), vec![medium]);
    }

    #[test]
    fn escalate_due_with_bad_now_ts_returns_empty() {
        let mut m = AlarmManager::new();
        m.raise("node-1", "fall", AlarmPriority::High, T0);
        assert!(m.escalate_due("not-a-timestamp", 1, 1).is_empty());
    }

    #[test]
    fn clear_marks_cleared_and_removes_from_active() {
        let mut m = AlarmManager::new();
        let id = m.raise("node-1", "fall", AlarmPriority::High, T0);
        assert_eq!(m.active().len(), 1);

        let ok = m.clear(id, T2);
        assert!(ok);
        // Removed from the active set.
        assert!(m.active().is_empty());
        // Clearing again fails (already cleared / gone).
        assert!(!m.clear(id, T2));

        // Audit records the clear with the cleared alarm's id.
        assert!(m.audit().iter().any(|e| e.alarm_id == id && e.action == audit_action::CLEARED));
    }

    #[test]
    fn high_alarm_latches_until_explicit_clear() {
        // Latching (Clause 6.11.2.2): a HIGH alarm stays out of the active set
        // ONLY after an explicit clear. Re-raising the same condition while it
        // is still active must dedupe rather than create a second alarm, and the
        // alarm never auto-clears on its own.
        let mut m = AlarmManager::new();
        let id = m.raise("node-1", "no-breathing", AlarmPriority::High, T0);
        // Condition "resolves" then re-fires — still the same latched alarm.
        let again = m.raise("node-1", "no-breathing", AlarmPriority::High, T1);
        assert_eq!(id, again);
        assert_eq!(m.active().len(), 1);
        // Only an explicit clear removes it.
        assert!(m.clear(id, T2));
        assert!(m.active().is_empty());
        // After clearing, the same condition can raise a fresh alarm with a new id.
        let fresh = m.raise("node-1", "no-breathing", AlarmPriority::High, T2);
        assert_ne!(fresh, id);
    }

    #[test]
    fn audit_log_grows_by_one_per_state_action() {
        let mut m = AlarmManager::new();
        let mut expected = 0;

        let id = m.raise("node-1", "fall", AlarmPriority::High, T0);
        expected += 1; // raised
        assert_eq!(m.audit().len(), expected);

        m.acknowledge(id, "nurse", T1);
        expected += 1; // acknowledged
        assert_eq!(m.audit().len(), expected);

        m.silence(id, "at bedside", T1);
        expected += 1; // silenced
        assert_eq!(m.audit().len(), expected);

        m.clear(id, T2);
        expected += 1; // cleared
        assert_eq!(m.audit().len(), expected);
    }

    #[test]
    fn alarm_serializes_to_json() {
        let mut m = AlarmManager::new();
        let id = m.raise("node-1", "fall", AlarmPriority::High, T0);
        let alarm = m.active().into_iter().find(|a| a.id == id).unwrap();
        let json = serde_json::to_string(alarm).unwrap();
        assert!(json.contains("\"priority\":\"High\""));
        assert!(json.contains("\"state\":\"Active\""));
        assert!(json.contains("\"source\":\"node-1\""));

        // Audit entries serialize too.
        let audit_json = serde_json::to_string(m.audit()).unwrap();
        assert!(audit_json.contains(audit_action::RAISED));
    }
}
