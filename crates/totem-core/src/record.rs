//! The common memory record shape: identity, content, provenance, economics,
//! and governance (docs/solution-intent.md §2.1).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::category::{MemoryCategory, ReviewPolicy};
use crate::ids::{IdError, MemoryId};
use crate::provenance::Provenance;
use crate::scope::Scope;

/// A lifecycle rule that refused an operation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleError {
    /// The category is append-only, so its records can never be rewritten.
    #[error("{0:?} records are append-only and cannot be revised")]
    AppendOnly(MemoryCategory),
}

/// The kind of thing a memory is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    /// An enrolled repository.
    Repo,
    /// An ARRIVE system.
    System,
    /// An ARRIVE component.
    Component,
    /// An ARRIVE advance.
    Advance,
    /// A person or agent.
    Actor,
    /// Another memory record.
    Memory,
}

/// A link from a memory to the entity or ARRIVE artifact it concerns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubjectRef {
    /// What kind of thing is referenced.
    pub kind: SubjectKind,
    /// The referenced id, in that kind's own namespace.
    pub id: String,
}

impl SubjectRef {
    /// Build a reference, rejecting an empty or untrimmed id.
    pub fn new(kind: SubjectKind, id: impl Into<String>) -> Result<Self, IdError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(IdError::Empty { kind: "subject" });
        }
        if id.trim() != id {
            return Err(IdError::Untrimmed {
                kind: "subject",
                value: id,
            });
        }
        Ok(Self { kind, id })
    }
}

/// What a memory says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Content {
    /// The memory itself, as written.
    pub body: String,
    /// The embedding used for vector recall, once one exists (ADV-STORE-002).
    ///
    /// Omitted from JSON entirely when absent (ADV-GATEWAY-014): client
    /// responses strip it, and a rendered `"embedding": null` on every record
    /// is noise on the most frequent call an agent makes. The store persists
    /// this field through `totem-store`'s own row mapping, not through serde,
    /// so this affects wire output only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Free-form tags for filtering.
    pub tags: Vec<String>,
}

impl Content {
    /// Content with no embedding and no tags yet.
    pub fn new(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            embedding: None,
            tags: Vec::new(),
        }
    }

    /// Attach tags.
    pub fn with_tags<T: Into<String>>(mut self, tags: impl IntoIterator<Item = T>) -> Self {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }
}

/// Usage counters and the value/currency terms that weight retrieval.
///
/// This advance defines the fields and their starting values; the scoring and
/// decay maths belong to ADV-CORE-002.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Economics {
    /// How many times the memory has been retrieved.
    pub use_count: u64,
    /// When it was last retrieved.
    pub last_used_at: Option<DateTime<Utc>>,
    /// How much the memory has earned its keep; a neutral multiplier at 1.0.
    pub value_score: f32,
    /// Freshness, decaying with time and refreshed on reinforcement.
    pub currency: f32,
}

impl Economics {
    /// The starting position for a memory nobody has read yet: unused, and
    /// neither favoured nor penalised in ranking.
    pub fn fresh() -> Self {
        Self {
            use_count: 0,
            last_used_at: None,
            value_score: 1.0,
            currency: 1.0,
        }
    }
}

/// Whether a memory is trusted right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    /// In force and eligible for retrieval.
    Active,
    /// Contradicted, with an Uncertainty record open against it.
    Contested,
    /// Withdrawn from retrieval but retained for audit.
    Retired,
}

/// Where a memory sits in human review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    /// The category needs no human gate.
    NotRequired,
    /// Awaiting a human decision.
    Pending,
    /// A human approved it.
    Approved,
    /// A human rejected it.
    Rejected,
}

/// A resolution that could not be recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GovernanceError {
    /// A decision must land on `Approved` or `Rejected`; nothing else answers
    /// "what did the human decide?".
    #[error("a review decision must be approved or rejected, not {0:?}")]
    NotADecision(ReviewState),
    /// The review is not open: either it never needed a human, or it was
    /// already decided. A second decision would put a contradiction in the
    /// record's own governance rather than a fresh fact.
    #[error("only a pending review can be resolved, and this one is {0:?}")]
    NotPending(ReviewState),
}

/// Review and status governing a memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Governance {
    /// Whether the memory is active, contested, or retired.
    pub status: MemoryStatus,
    /// Where it sits in human review.
    pub review: ReviewState,
}

impl Governance {
    /// The starting position for a new record of the given category: active,
    /// and pending review only where the category is human-gated.
    pub fn initial(category: MemoryCategory) -> Self {
        let review = match category.lifecycle().review {
            ReviewPolicy::Automatic => ReviewState::NotRequired,
            ReviewPolicy::HumanGated => ReviewState::Pending,
        };
        Self {
            status: MemoryStatus::Active,
            review,
        }
    }

    /// Record a human's decision on a pending review (ADV-CONSOLE-002: the
    /// Uncertainty queue's resolution).
    ///
    /// Refused unless the review is currently [`ReviewState::Pending`] — a
    /// decision, once recorded, is not reopened, the same "no second
    /// decision" rule [`crate::PromotionEvent`] enforces for a promotion
    /// proposal — and unless `decision` is itself a decision
    /// ([`ReviewState::Approved`] or [`ReviewState::Rejected`]), so a caller
    /// cannot "resolve" a review back into `Pending` or `NotRequired`.
    pub fn resolve(self, decision: ReviewState) -> Result<Self, GovernanceError> {
        if !matches!(decision, ReviewState::Approved | ReviewState::Rejected) {
            return Err(GovernanceError::NotADecision(decision));
        }
        if self.review != ReviewState::Pending {
            return Err(GovernanceError::NotPending(self.review));
        }
        Ok(Self {
            review: decision,
            ..self
        })
    }
}

/// One memory: exactly one category, at exactly one scope, with provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecord {
    /// This record's identity.
    pub id: MemoryId,
    /// Its category, which decides its lifecycle.
    pub category: MemoryCategory,
    /// The isolation boundary it belongs to.
    pub scope: Scope,
    /// The entity or ARRIVE artifact it concerns, if any.
    pub subject: Option<SubjectRef>,
    /// What it says.
    pub content: Content,
    /// Who wrote it, from where, and when.
    pub provenance: Provenance,
    /// Usage and value terms.
    pub economics: Economics,
    /// Status and review state.
    pub governance: Governance,
}

impl MemoryRecord {
    /// Write a new record. Provenance is a parameter, not a default, so no
    /// record can exist without an author, harness, session, and time.
    pub fn new(
        category: MemoryCategory,
        scope: Scope,
        content: Content,
        provenance: Provenance,
    ) -> Self {
        Self {
            id: MemoryId::new(),
            category,
            scope,
            subject: None,
            content,
            provenance,
            economics: Economics::fresh(),
            governance: Governance::initial(category),
        }
    }

    /// Replace the record's content.
    ///
    /// Refused for append-only categories: rewriting an episode would destroy
    /// the substrate every other memory's lineage is reconstructed from.
    pub fn revise(&mut self, content: Content) -> Result<(), LifecycleError> {
        if self.category.is_append_only() {
            return Err(LifecycleError::AppendOnly(self.category));
        }
        self.content = content;
        Ok(())
    }

    /// When this record lapses, from its category's default TTL. `None` means
    /// it lives until it is retired.
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.category
            .lifecycle()
            .default_ttl
            .map(|ttl| self.provenance.created_at + ttl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ActorId, RepoId, SessionId};
    use crate::provenance::{Author, Harness};

    fn record(category: MemoryCategory) -> MemoryRecord {
        MemoryRecord::new(
            category,
            Scope::Project(RepoId::new("srswart/totem").expect("valid repo id")),
            Content::new("body"),
            Provenance::new(
                Author::Human(ActorId::new("ada").expect("valid actor id")),
                Harness::Console,
                SessionId::new("sess-1").expect("valid session id"),
                "2026-08-05T06:00:00Z".parse().expect("valid timestamp"),
            ),
        )
    }

    #[test]
    fn every_category_can_be_written_and_only_episodic_refuses_revision() {
        for category in MemoryCategory::ALL {
            let mut written = record(category);
            let revised = written.revise(Content::new("revised"));
            assert_eq!(revised.is_err(), category.is_append_only(), "{category:?}");
        }
    }

    #[test]
    fn human_gated_categories_start_pending_review() {
        for category in MemoryCategory::ALL {
            let expected = match category.lifecycle().review {
                ReviewPolicy::Automatic => ReviewState::NotRequired,
                ReviewPolicy::HumanGated => ReviewState::Pending,
            };
            assert_eq!(record(category).governance.review, expected, "{category:?}");
        }
    }

    #[test]
    fn each_record_gets_its_own_identity() {
        assert_ne!(
            record(MemoryCategory::Knowledge).id,
            record(MemoryCategory::Knowledge).id
        );
    }

    #[test]
    fn subject_refs_reject_blank_ids() {
        assert!(SubjectRef::new(SubjectKind::Component, "  ").is_err());
        assert!(SubjectRef::new(SubjectKind::Component, " core").is_err());
        assert!(SubjectRef::new(SubjectKind::Component, "core").is_ok());
        assert_eq!(record(MemoryCategory::Context).subject, None);
    }

    #[test]
    fn a_pending_review_resolves_to_either_decision() {
        for decision in [ReviewState::Approved, ReviewState::Rejected] {
            let pending = Governance {
                status: MemoryStatus::Active,
                review: ReviewState::Pending,
            };
            let resolved = pending
                .resolve(decision)
                .expect("a pending review resolves");
            assert_eq!(resolved.review, decision);
            assert_eq!(
                resolved.status, pending.status,
                "resolving a review does not touch status"
            );
        }
    }

    #[test]
    fn resolving_refuses_a_target_that_is_not_a_decision() {
        let pending = Governance {
            status: MemoryStatus::Active,
            review: ReviewState::Pending,
        };
        for not_a_decision in [ReviewState::NotRequired, ReviewState::Pending] {
            assert_eq!(
                pending.resolve(not_a_decision),
                Err(GovernanceError::NotADecision(not_a_decision))
            );
        }
    }

    #[test]
    fn resolving_refuses_a_review_that_is_not_pending() {
        for already in [
            ReviewState::NotRequired,
            ReviewState::Approved,
            ReviewState::Rejected,
        ] {
            let governance = Governance {
                status: MemoryStatus::Active,
                review: already,
            };
            assert_eq!(
                governance.resolve(ReviewState::Approved),
                Err(GovernanceError::NotPending(already))
            );
        }
    }
}
