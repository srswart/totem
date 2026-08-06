//! Access log entries: one per read or write, so any memory's access history
//! is reconstructable (docs/project-brief.md G3; ADV-GATEWAY-001).
//!
//! An entry is deliberately not a [`crate::MemoryRecord`] — it is not memory a
//! reader recalls, it is the audit trail *of* recalling and saving memory, and
//! it must exist even for callers who touch nothing (a denied write is still
//! worth a record, though this type only carries what a *successful* operation
//! did; the caller decides whether to log a refusal).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, MemoryId, SessionId};
use crate::provenance::Harness;

/// Which operation an access log entry records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessOperation {
    /// A read of the merged, scope-resolved view.
    Recall,
    /// A write of a new memory record.
    Save,
}

/// Who touched memory, from where, when, and via which surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccessLogEntry {
    /// The caller's own identity — never another actor's, since it is always
    /// read from the caller's own request, the same source a [`crate::ScopeChain`]
    /// is resolved from.
    pub actor: ActorId,
    /// Which harness the access arrived through.
    pub harness: Harness,
    /// The harness session the access belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
    /// Which operation this entry records.
    pub operation: AccessOperation,
    /// Which surface handled the request, e.g. `/recall` or `/save`.
    pub endpoint: String,
    /// The record written, for a [`AccessOperation::Save`] entry.
    pub memory_id: Option<MemoryId>,
    /// How many records a [`AccessOperation::Recall`] entry returned.
    pub result_count: Option<u64>,
    /// When the access happened.
    pub at: DateTime<Utc>,
}

impl AccessLogEntry {
    /// Record an access. The identity, operation, endpoint, and time are
    /// mandatory — there is no way to build an entry that cannot answer "who,
    /// what, when, via which endpoint, in which session".
    pub fn new(
        actor: ActorId,
        harness: Harness,
        session: SessionId,
        operation: AccessOperation,
        endpoint: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Self {
        Self {
            actor,
            harness,
            session,
            turn: None,
            operation,
            endpoint: endpoint.into(),
            memory_id: None,
            result_count: None,
            at,
        }
    }

    /// Attach the turn within the session.
    pub fn at_turn(mut self, turn: u32) -> Self {
        self.turn = Some(turn);
        self
    }

    /// Attach the record a [`AccessOperation::Save`] entry wrote.
    pub fn for_memory(mut self, id: MemoryId) -> Self {
        self.memory_id = Some(id);
        self
    }

    /// Attach how many records a [`AccessOperation::Recall`] entry returned.
    pub fn with_result_count(mut self, count: u64) -> Self {
        self.result_count = Some(count);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(id: &str) -> ActorId {
        ActorId::new(id).expect("valid actor id")
    }

    fn base() -> AccessLogEntry {
        AccessLogEntry::new(
            actor("ada"),
            Harness::ClaudeCode,
            SessionId::new("sess-1").expect("valid session id"),
            AccessOperation::Recall,
            "/recall",
            "2026-08-05T06:00:00Z".parse().expect("valid timestamp"),
        )
    }

    #[test]
    fn a_new_entry_carries_no_memory_id_turn_or_result_count() {
        let entry = base();
        assert_eq!(entry.turn, None);
        assert_eq!(entry.memory_id, None);
        assert_eq!(entry.result_count, None);
    }

    #[test]
    fn builder_methods_attach_the_optional_fields() {
        let id = MemoryId::new();
        let entry = base().at_turn(3).for_memory(id).with_result_count(7);
        assert_eq!(entry.turn, Some(3));
        assert_eq!(entry.memory_id, Some(id));
        assert_eq!(entry.result_count, Some(7));
    }

    #[test]
    fn an_entry_round_trips_through_json() {
        let original = base().at_turn(1).with_result_count(2);
        let json = serde_json::to_string(&original).expect("serialises");
        let back: AccessLogEntry = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back, original);
    }
}
