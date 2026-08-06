//! Curation: what a maintenance agent may do to memory it did not write.
//!
//! Totem's curators are background agents that tidy memory through the same
//! core API everyone else uses (docs/solution-intent.md §5). What makes them
//! safe is not that they are careful but that their one action is
//! *supersession*: the original stays, marked [`MemoryStatus::Retired`], and a
//! superseding record cites it. The component invariant says it plainly —
//! "curation never deletes; originals are superseded, every action is logged
//! and reversible" (`components/curator.yaml`).
//!
//! Three rules do the work here, and each exists because the alternative is a
//! failure nobody would notice:
//!
//! - **A merge never crosses a scope boundary.** Merging a private note into a
//!   project record would publish it with no promotion event and nobody's
//!   approval — the project's highest-severity failure, arrived at sideways.
//!   Crossing scopes is promotion's job (`crate::promotion`), and promotion is
//!   a decision with an author.
//! - **A curator only acts alone where a human never had to.** The categories
//!   whose [`ReviewPolicy`] is `Automatic` are documented as the ones curators
//!   may act on directly; the human-gated ones stay human-gated, and the
//!   append-only ones cannot be touched at all.
//! - **Every event carries what it would take to undo it.** A
//!   [`CurationEvent`] records each original *and the status it held*, so a
//!   rollback restores what was there rather than assuming everything was
//!   active.
//!
//! This module decides *what may happen*. Applying it to real rows — retiring
//! exactly the records the event names, in one transaction, and putting them
//! back on rollback — is `totem-store`'s job.

use crate::category::{MemoryCategory, ReviewPolicy};
use crate::ids::{CurationId, MemoryId};
use crate::provenance::Provenance;
use crate::record::{MemoryRecord, MemoryStatus};
use crate::scope::Scope;

/// The smallest group a merge can collapse.
///
/// One original is not a duplicate — replacing a single record with a copy of
/// itself is a rewrite wearing a merge's clothes, and revision already has an
/// honest path (`MemoryRecord::revise`).
const MINIMUM_MERGE_GROUP: usize = 2;

/// Why a proposed curation was refused before any record was touched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CurationError {
    /// The category is one no curator may act on without a human.
    #[error("{0:?} memory may not be curated without a human decision")]
    Forbidden(MemoryCategory),
    /// A merge named records at more than one scope.
    #[error("a merge may not cross a scope boundary: {merged} would absorb {superseded}")]
    CrossScope {
        /// Where the superseding record sits.
        merged: Scope,
        /// The offending original's scope.
        superseded: Scope,
    },
    /// A merge named records of more than one category.
    #[error("a merge may not mix categories: {merged:?} would absorb {superseded:?}")]
    MixedCategory {
        /// The superseding record's category.
        merged: MemoryCategory,
        /// The offending original's category.
        superseded: MemoryCategory,
    },
    /// Fewer originals than it takes to have a duplicate at all.
    #[error("a merge needs at least {MINIMUM_MERGE_GROUP} originals, but {superseded} were named")]
    NothingToMerge {
        /// How many originals were named.
        superseded: usize,
    },
    /// The superseding record was named among the records it supersedes.
    #[error("memory {0} cannot supersede itself")]
    SelfSupersede(MemoryId),
    /// An original was already retired, so a previous curation has superseded
    /// it and this one would be superseding a tombstone.
    #[error("memory {0} is not active and cannot be superseded")]
    NotActive(MemoryId),
    /// A rollback named an event that is not a merge.
    #[error("curation event {0} is not a merge and cannot be rolled back")]
    NotAMerge(CurationId),
}

/// What a recorded curation event says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CurationEventKind {
    /// A curator wrote a superseding record and retired the originals.
    Merged,
    /// A merge was undone: the originals are active again and the superseding
    /// record is retired in their place.
    RolledBack,
}

/// One original a merge superseded, and the status it held at the time.
///
/// The status is recorded rather than assumed because rollback restores it: an
/// event that only remembered *which* records it retired could only ever put
/// them back as active, quietly promoting a contested record to a trusted one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Supersession {
    /// The superseded record.
    pub memory: MemoryId,
    /// What its status was before the merge.
    pub prior_status: MemoryStatus,
}

/// One recorded curator action.
///
/// Constructed only through [`CurationPolicy::merge`] and
/// [`CurationEvent::rolled_back`], so an event cannot name a scope, a set of
/// originals, or a prior status that disagrees with the records it was built
/// from — there is no constructor that takes those separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurationEvent {
    /// This event's identity.
    pub id: CurationId,
    /// What happened.
    pub kind: CurationEventKind,
    /// The superseding record: written by a merge, retired by its rollback.
    pub merged: MemoryId,
    /// The one scope every record in this event sits at.
    pub scope: Scope,
    /// The originals and the statuses they held before the merge.
    pub superseded: Vec<Supersession>,
    /// The merge a rollback undoes; `None` on a merge.
    pub rolls_back: Option<CurationId>,
    /// Why, in the words of whoever acted.
    pub reason: Option<String>,
    /// Which curator acted, from which session, and when.
    pub provenance: Provenance,
}

impl CurationEvent {
    /// The originals this event covers, in the order they were named.
    pub fn superseded_ids(&self) -> Vec<MemoryId> {
        self.superseded
            .iter()
            .map(|supersession| supersession.memory)
            .collect()
    }

    /// Undo a merge: the same originals, restored to the statuses this event
    /// recorded, and the superseding record retired in exchange.
    ///
    /// Refused for anything but a merge. Rolling back a rollback would put a
    /// second, contradictory restoration in the trail; the way to re-merge is a
    /// new merge, which is a new decision with its own event and author.
    pub fn rolled_back(&self, provenance: Provenance) -> Result<Self, CurationError> {
        if self.kind != CurationEventKind::Merged {
            return Err(CurationError::NotAMerge(self.id));
        }
        Ok(Self {
            id: CurationId::new(),
            kind: CurationEventKind::RolledBack,
            merged: self.merged,
            scope: self.scope.clone(),
            superseded: self.superseded.clone(),
            rolls_back: Some(self.id),
            reason: None,
            provenance,
        })
    }

    /// Attach the reason whoever acted gave.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

/// Which memory a curator may consolidate, and on what terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurationPolicy {
    _private: (),
}

impl Default for CurationPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl CurationPolicy {
    /// The standing policy.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Whether a curator may act on this category without a human.
    ///
    /// Deliberately not a second table: it reads the category's existing
    /// [`ReviewPolicy`], whose `Automatic` variant is documented as "no human
    /// gate; curators may act on it directly". A category cannot be
    /// human-gated for review and quietly curatable in the background.
    pub fn may_curate(&self, category: MemoryCategory) -> bool {
        !category.is_append_only() && category.lifecycle().review == ReviewPolicy::Automatic
    }

    /// Check a proposed merge and, if it holds, mint the event that records it.
    ///
    /// The event is built from the records themselves, so the scope it claims
    /// and the prior statuses it will restore are the ones the store actually
    /// read — not a caller's account of them.
    pub fn merge(
        &self,
        merged: &MemoryRecord,
        superseded: &[MemoryRecord],
        provenance: Provenance,
    ) -> Result<CurationEvent, CurationError> {
        if !self.may_curate(merged.category) {
            return Err(CurationError::Forbidden(merged.category));
        }
        if superseded.len() < MINIMUM_MERGE_GROUP {
            return Err(CurationError::NothingToMerge {
                superseded: superseded.len(),
            });
        }

        for original in superseded {
            if original.id == merged.id {
                return Err(CurationError::SelfSupersede(merged.id));
            }
            if original.category != merged.category {
                return Err(CurationError::MixedCategory {
                    merged: merged.category,
                    superseded: original.category,
                });
            }
            if original.scope != merged.scope {
                return Err(CurationError::CrossScope {
                    merged: merged.scope.clone(),
                    superseded: original.scope.clone(),
                });
            }
            if original.governance.status == MemoryStatus::Retired {
                return Err(CurationError::NotActive(original.id));
            }
        }

        Ok(CurationEvent {
            id: CurationId::new(),
            kind: CurationEventKind::Merged,
            merged: merged.id,
            scope: merged.scope.clone(),
            superseded: superseded
                .iter()
                .map(|original| Supersession {
                    memory: original.id,
                    prior_status: original.governance.status,
                })
                .collect(),
            rolls_back: None,
            reason: None,
            provenance,
        })
    }
}

#[cfg(test)]
mod tests {
    //! The category rule, which the integration tests reach only through
    //! whichever categories happen to be curatable today.

    use super::*;

    #[test]
    fn curatable_categories_are_exactly_the_revisable_automatic_ones() {
        let policy = CurationPolicy::new();
        for category in MemoryCategory::ALL {
            let expected = !category.is_append_only()
                && category.lifecycle().review == ReviewPolicy::Automatic;
            assert_eq!(policy.may_curate(category), expected, "{category:?}");
        }
        assert!(policy.may_curate(MemoryCategory::Knowledge));
        assert!(!policy.may_curate(MemoryCategory::Episodic));
        assert!(!policy.may_curate(MemoryCategory::Instructions));
    }
}
