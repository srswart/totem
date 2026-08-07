//! Access log entries: one per read or write, so any memory's access history
//! is reconstructable (docs/project-brief.md G3; ADV-GATEWAY-001) — and, since
//! ADV-CORE-006, one per *refused* request too: an attempt is worth an audit
//! record even though it touched no memory.
//!
//! An entry is deliberately not a [`crate::MemoryRecord`] — it is not memory a
//! reader recalls, it is the audit trail *of* recalling and saving memory.

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
    /// An explicit feedback signal applied to an existing record's economics
    /// (ADV-GATEWAY-004 gap-fill) — distinct from [`AccessOperation::Save`]
    /// because it revises a record's economics rather than writing a new one.
    Feedback,
    /// A scope promotion or demotion was asked for (ADV-CONSOLE-002): the
    /// write side of `promotion_event`, distinct from [`AccessOperation::Save`]
    /// because the memory it names already existed.
    Propose,
    /// A human decided a queued promotion proposal, approving or rejecting it
    /// (ADV-CONSOLE-002).
    PromotionDecision,
    /// A human resolved a pending Uncertainty review, approving or rejecting
    /// it (ADV-CONSOLE-002) — the governance-state twin of
    /// [`AccessOperation::Feedback`].
    Resolve,
    /// A request was refused before it reached the store — an authentication
    /// or authorization failure, not a completed read or write (ADV-CORE-006).
    Refused,
}

/// Why a request was refused, for a [`AccessOperation::Refused`] entry
/// (ADV-CORE-006).
///
/// Deliberately its own vocabulary rather than a re-export of a surface's own
/// error type: `totem-core` sits beneath every surface that can refuse a
/// request (`totem-gateway`'s `AuthError` today, and it is not the last),
/// so the reason has to outlive any one of them. The split mirrors the one
/// every refusing surface already makes: "we do not know who you are"
/// ([`RefusalReason::MissingCredential`], [`RefusalReason::UnknownCredential`],
/// [`RefusalReason::Expired`]) versus "we know who you are, and this is
/// outside your grant" ([`RefusalReason::ActorNotBound`],
/// [`RefusalReason::RepoNotBound`], [`RefusalReason::ScopeNotBound`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalReason {
    /// No credential was presented at all.
    MissingCredential,
    /// The presented credential is not recognized (forged or revoked).
    UnknownCredential,
    /// The presented credential has expired.
    Expired,
    /// The request asserted an identity the credential is not bound to.
    ActorNotBound,
    /// The request named a repo the credential is not bound to.
    RepoNotBound,
    /// The request reached a scope outside the credential's binding.
    ScopeNotBound,
}

/// Who touched memory, from where, when, and via which surface — or, for a
/// [`AccessOperation::Refused`] entry, who a refusal could not confirm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccessLogEntry {
    /// The caller's own identity — never another actor's, since it is always
    /// read from the caller's own request, the same source a [`crate::ScopeChain`]
    /// is resolved from. `None` on a [`AccessOperation::Refused`] entry: there
    /// was no confirmed identity to record.
    pub actor: Option<ActorId>,
    /// Which harness the access arrived through. `None` on a refusal that
    /// never reached the point where a harness is known.
    pub harness: Option<Harness>,
    /// The harness session the access belongs to. `None` on a refusal that
    /// never reached the point where a session is known.
    pub session: Option<SessionId>,
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
    /// Why a [`AccessOperation::Refused`] entry was refused. `None` for
    /// every other operation.
    pub refusal_reason: Option<RefusalReason>,
    /// A hex-encoded fingerprint of the credential presented on a
    /// [`AccessOperation::Refused`] entry, when one was presented at all —
    /// never the token text itself. `None` for every other operation, and for
    /// a refusal that had no credential to fingerprint (e.g.
    /// [`RefusalReason::MissingCredential`]).
    pub credential_fingerprint: Option<String>,
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
            actor: Some(actor),
            harness: Some(harness),
            session: Some(session),
            turn: None,
            operation,
            endpoint: endpoint.into(),
            memory_id: None,
            result_count: None,
            refusal_reason: None,
            credential_fingerprint: None,
            at,
        }
    }

    /// Record a refusal (ADV-CORE-006): a request turned away before it
    /// reached the store, so there is no confirmed identity, turn, memory, or
    /// result count to attach — only why, via which endpoint, and when.
    pub fn refused(reason: RefusalReason, endpoint: impl Into<String>, at: DateTime<Utc>) -> Self {
        Self {
            actor: None,
            harness: None,
            session: None,
            turn: None,
            operation: AccessOperation::Refused,
            endpoint: endpoint.into(),
            memory_id: None,
            result_count: None,
            refusal_reason: Some(reason),
            credential_fingerprint: None,
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

    /// Attach the fingerprint of the credential a refused request presented,
    /// when it presented one at all (ADV-CORE-006).
    pub fn with_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.credential_fingerprint = Some(fingerprint.into());
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

    // ADV-CORE-006: a refused request carries no identity — there was none to
    // confirm — but still names why and, when a credential was presented at
    // all, its fingerprint.
    #[test]
    fn a_refusal_entry_carries_no_identity_fields() {
        let entry = AccessLogEntry::refused(
            RefusalReason::MissingCredential,
            "/recall",
            "2026-08-05T06:00:00Z".parse().expect("valid timestamp"),
        );
        assert_eq!(entry.actor, None);
        assert_eq!(entry.harness, None);
        assert_eq!(entry.session, None);
        assert_eq!(entry.memory_id, None);
        assert_eq!(entry.result_count, None);
        assert_eq!(entry.operation, AccessOperation::Refused);
        assert_eq!(entry.refusal_reason, Some(RefusalReason::MissingCredential));
        assert_eq!(entry.credential_fingerprint, None);
    }

    #[test]
    fn with_fingerprint_attaches_the_presented_credentials_fingerprint() {
        let entry = AccessLogEntry::refused(
            RefusalReason::ScopeNotBound,
            "/save",
            "2026-08-05T06:00:00Z".parse().expect("valid timestamp"),
        )
        .with_fingerprint("deadbeef");
        assert_eq!(entry.credential_fingerprint, Some("deadbeef".to_string()));
    }

    #[test]
    fn a_refusal_entry_round_trips_through_json() {
        let original = AccessLogEntry::refused(
            RefusalReason::ActorNotBound,
            "/recall",
            "2026-08-05T06:00:00Z".parse().expect("valid timestamp"),
        )
        .with_fingerprint("deadbeef");
        let json = serde_json::to_string(&original).expect("serialises");
        let back: AccessLogEntry = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back, original);
    }
}
