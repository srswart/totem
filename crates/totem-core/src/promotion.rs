//! Promotion: the one sanctioned path across a scope boundary.
//!
//! Sharing in Totem is by promotion, not by default (docs/solution-intent.md
//! §2.2). A memory written at `actor` scope can be *proposed* for a wider
//! scope; policy decides whether that proposal takes effect at once or waits
//! for a human; and every step — the ask, the decision, and the scope change
//! it caused — is a recorded event carrying full [`Provenance`].
//!
//! Two things follow from that and are worth saying plainly, because they are
//! what makes promotion auditable rather than merely convenient:
//!
//! - There is no in-place scope edit anywhere. `MemoryRecord` exposes no way
//!   to change `scope`, and `totem-store`'s only statement that writes it runs
//!   inside the transaction that records the event authorising it.
//! - **Demotion** is the compensating event, and it is never gated. Narrowing
//!   reduces exposure, so a bad promotion can always be walked back without
//!   waiting for the same queue that let it through.
//!
//! This module decides *what may happen*. Enforcing it against real records —
//! and checking that the caller can reach either end of the move — is
//! `totem-store`'s job.

use crate::category::{MemoryCategory, ReviewPolicy};
use crate::ids::{MemoryId, PromotionId};
use crate::provenance::Provenance;
use crate::scope::Scope;

/// How a proposed scope change is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromotionPath {
    /// Policy allows it outright: proposing it promotes it.
    Automatic,
    /// A human must approve before the record moves.
    HumanGated,
    /// The category may never change scope at all.
    Forbidden,
}

/// Why a proposed scope change was refused before any record was touched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PromotionError {
    /// The category's records may never change scope.
    #[error("{0:?} memory is append-only and can never change scope")]
    Forbidden(MemoryCategory),
    /// A promotion named a target that is not strictly wider than the origin.
    #[error("promotion must widen scope, but {to} is not wider than {from}")]
    NotWidening {
        /// Where the record sits now.
        from: Scope,
        /// Where the proposal wanted it.
        to: Scope,
    },
    /// A demotion named a target that is not strictly narrower than the origin.
    #[error("demotion must narrow scope, but {to} is not narrower than {from}")]
    NotNarrowing {
        /// Where the record sits now.
        from: Scope,
        /// Where the demotion wanted it.
        to: Scope,
    },
}

/// Which categories may cross a scope boundary, and who has to agree.
///
/// The per-category gate is deliberately not a second table: it reads the
/// category's existing [`ReviewPolicy`], so a category cannot be human-gated
/// for review and quietly automatic for sharing. Append-only categories are
/// [`PromotionPath::Forbidden`] outright — an episodic row cannot be touched at
/// all, so moving one is impossible rather than merely gated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromotionPolicy {
    human_gated_everywhere: bool,
}

impl Default for PromotionPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl PromotionPolicy {
    /// The standing policy: each category takes the path its own review policy
    /// implies.
    pub fn new() -> Self {
        Self {
            human_gated_everywhere: false,
        }
    }

    /// The tightened policy: every category that *could* be gated is, whatever
    /// its own review policy says.
    ///
    /// This is the configuration lever the advance's rollback plan names. It
    /// only ever tightens — a forbidden category stays forbidden.
    pub fn human_gated_everywhere() -> Self {
        Self {
            human_gated_everywhere: true,
        }
    }

    /// How a promotion of this category is decided, ignoring the scopes.
    pub fn path(&self, category: MemoryCategory) -> PromotionPath {
        if category.is_append_only() {
            return PromotionPath::Forbidden;
        }
        if self.human_gated_everywhere {
            return PromotionPath::HumanGated;
        }
        match category.lifecycle().review {
            ReviewPolicy::Automatic => PromotionPath::Automatic,
            ReviewPolicy::HumanGated => PromotionPath::HumanGated,
        }
    }

    /// Check a proposed promotion, returning the path it must take.
    ///
    /// A promotion must strictly widen. Equal scopes are refused too: recording
    /// an approval for a move that changes nothing would put an untrue event in
    /// the audit trail.
    pub fn check_promotion(
        &self,
        category: MemoryCategory,
        from: &Scope,
        to: &Scope,
    ) -> Result<PromotionPath, PromotionError> {
        let path = self.path(category);
        if path == PromotionPath::Forbidden {
            return Err(PromotionError::Forbidden(category));
        }
        if to.specificity() <= from.specificity() {
            return Err(PromotionError::NotWidening {
                from: from.clone(),
                to: to.clone(),
            });
        }
        Ok(path)
    }

    /// Check a proposed demotion.
    ///
    /// Unlike promotion this takes no path: narrowing is the rollback lever, so
    /// it is available immediately to anyone who could have promoted in the
    /// first place. Only the append-only categories are refused.
    pub fn check_demotion(
        &self,
        category: MemoryCategory,
        from: &Scope,
        to: &Scope,
    ) -> Result<(), PromotionError> {
        if category.is_append_only() {
            return Err(PromotionError::Forbidden(category));
        }
        if to.specificity() >= from.specificity() {
            return Err(PromotionError::NotNarrowing {
                from: from.clone(),
                to: to.clone(),
            });
        }
        Ok(())
    }
}

/// What a recorded promotion event says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromotionEventKind {
    /// Someone asked for the record to move.
    Proposed,
    /// Policy allowed the move without a human, and it happened.
    AutoApproved,
    /// A human allowed the move, and it happened.
    Approved,
    /// A human refused the move; the record did not go anywhere.
    Rejected,
    /// The record was narrowed back, compensating an earlier promotion.
    Demoted,
}

impl PromotionEventKind {
    /// Whether this kind of event answers an outstanding proposal.
    pub fn is_decision(self) -> bool {
        matches!(
            self,
            PromotionEventKind::AutoApproved
                | PromotionEventKind::Approved
                | PromotionEventKind::Rejected
        )
    }
}

/// One recorded step in a record's scope history.
///
/// Constructed only through the methods below, so a decision can never name a
/// different record, origin, or target than the proposal it answers — there is
/// no constructor that takes those separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionEvent {
    /// This event's identity.
    pub id: PromotionId,
    /// The record whose scope is at stake.
    pub memory: MemoryId,
    /// What happened.
    pub kind: PromotionEventKind,
    /// Where the record sat when this event was written.
    pub from_scope: Scope,
    /// Where it was asked to go, or where a demotion put it.
    pub to_scope: Scope,
    /// The proposal a decision answers; `None` on a proposal or a demotion.
    pub proposal: Option<PromotionId>,
    /// Why, in the words of whoever acted.
    pub reason: Option<String>,
    /// Who acted, from which harness and session, and when.
    pub provenance: Provenance,
}

impl PromotionEvent {
    /// Ask for a record to move to a wider scope.
    pub fn propose(
        memory: MemoryId,
        from_scope: Scope,
        to_scope: Scope,
        provenance: Provenance,
    ) -> Self {
        Self {
            id: PromotionId::new(),
            memory,
            kind: PromotionEventKind::Proposed,
            from_scope,
            to_scope,
            proposal: None,
            reason: None,
            provenance,
        }
    }

    /// Narrow a record back. No proposal precedes a demotion: narrowing is
    /// always available, so there is nothing to queue.
    pub fn demotion(
        memory: MemoryId,
        from_scope: Scope,
        to_scope: Scope,
        provenance: Provenance,
    ) -> Self {
        Self {
            kind: PromotionEventKind::Demoted,
            ..Self::propose(memory, from_scope, to_scope, provenance)
        }
    }

    /// A human's approval of this proposal.
    pub fn approved(&self, provenance: Provenance) -> Self {
        self.decide(PromotionEventKind::Approved, provenance)
    }

    /// Policy's own approval of this proposal, recorded distinctly from a
    /// human's so the audit trail never has to guess which one it was.
    pub fn auto_approved(&self, provenance: Provenance) -> Self {
        self.decide(PromotionEventKind::AutoApproved, provenance)
    }

    /// A human's refusal of this proposal.
    pub fn rejected(&self, provenance: Provenance) -> Self {
        self.decide(PromotionEventKind::Rejected, provenance)
    }

    fn decide(&self, kind: PromotionEventKind, provenance: Provenance) -> Self {
        Self {
            id: PromotionId::new(),
            memory: self.memory,
            kind,
            from_scope: self.from_scope.clone(),
            to_scope: self.to_scope.clone(),
            proposal: Some(self.id),
            reason: None,
            provenance,
        }
    }

    /// Attach the reason whoever acted gave.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Whether this event is one that actually moves the record.
    ///
    /// The store writes the scope change and the event in a single
    /// transaction, so this is also the set of events that can only exist if
    /// the move really happened.
    pub fn takes_effect(&self) -> bool {
        matches!(
            self.kind,
            PromotionEventKind::AutoApproved
                | PromotionEventKind::Approved
                | PromotionEventKind::Demoted
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ActorId, RepoId, SessionId};
    use crate::provenance::{Author, Harness};

    fn provenance() -> Provenance {
        Provenance::new(
            Author::Human(ActorId::new("ada").expect("valid actor id")),
            Harness::Console,
            SessionId::new("sess-1").expect("valid session id"),
            "2026-08-06T06:00:00Z".parse().expect("valid timestamp"),
        )
    }

    fn private() -> Scope {
        Scope::Actor(ActorId::new("ada").expect("valid actor id"))
    }

    fn project() -> Scope {
        Scope::Project(RepoId::new("srswart/totem").expect("valid repo id"))
    }

    #[test]
    fn a_demotion_answers_no_proposal() {
        let event = PromotionEvent::demotion(MemoryId::new(), project(), private(), provenance());
        assert_eq!(event.proposal, None);
        assert!(event.takes_effect());
        assert!(!event.kind.is_decision());
    }

    #[test]
    fn every_decision_kind_reports_itself_as_one() {
        let proposal = PromotionEvent::propose(MemoryId::new(), private(), project(), provenance());
        for decision in [
            proposal.approved(provenance()),
            proposal.auto_approved(provenance()),
            proposal.rejected(provenance()),
        ] {
            assert!(decision.kind.is_decision(), "{:?}", decision.kind);
            assert_eq!(decision.proposal, Some(proposal.id));
        }
        assert!(!proposal.kind.is_decision());
    }
}
