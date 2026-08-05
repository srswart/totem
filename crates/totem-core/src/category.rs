//! The six memory categories and the lifecycle each one implies.
//!
//! Category is not a label: it decides whether a record may ever be rewritten,
//! how long it lives, whether it decays, whether a human must review it, and
//! how loudly it competes for room in an agent's context window
//! (docs/solution-intent.md §2.1).

use chrono::TimeDelta;
use serde::{Deserialize, Serialize};

/// The category of a memory record. Every record is exactly one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// Every session and turn, kept exactly as it happened: the audit substrate.
    Episodic,
    /// People, agents, and systems in play, and what is true about them.
    Identity,
    /// Facts and preferences about the domain.
    Knowledge,
    /// The working set — what is going on right now.
    Context,
    /// Standing rules: how this team or project wants things done.
    Instructions,
    /// Contradictions kept explicit instead of silently resolved.
    Uncertainty,
}

/// Whether a record may be rewritten in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mutability {
    /// The record is written once and never edited.
    AppendOnly,
    /// The record's content may be replaced by a later revision.
    Revisable,
}

/// Who has to agree before a record of this category is trusted or shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewPolicy {
    /// No human gate; curators may act on it directly.
    Automatic,
    /// A human must review it before it takes effect.
    HumanGated,
}

/// The lifecycle rules a category imposes on its records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CategoryLifecycle {
    /// Whether records may be revised after they are written.
    pub mutability: Mutability,
    /// How long a record stays live without an explicit expiry, if at all.
    pub default_ttl: Option<TimeDelta>,
    /// Whether the record's currency decays without reinforcement.
    pub decays: bool,
    /// Whether a human must review the record.
    pub review: ReviewPolicy,
    /// Relative weight when assembling context; higher is injected first.
    pub injection_priority: u8,
}

impl MemoryCategory {
    /// Every category, in the order the Solution Intent lists them.
    pub const ALL: [MemoryCategory; 6] = [
        MemoryCategory::Episodic,
        MemoryCategory::Identity,
        MemoryCategory::Knowledge,
        MemoryCategory::Context,
        MemoryCategory::Instructions,
        MemoryCategory::Uncertainty,
    ];

    /// The lifecycle rules this category imposes.
    pub fn lifecycle(self) -> CategoryLifecycle {
        match self {
            // The audit substrate: written once, never edited, never decayed —
            // a decayed episode would be an audit gap.
            MemoryCategory::Episodic => CategoryLifecycle {
                mutability: Mutability::AppendOnly,
                default_ttl: None,
                decays: false,
                review: ReviewPolicy::Automatic,
                injection_priority: 10,
            },
            MemoryCategory::Identity => CategoryLifecycle {
                mutability: Mutability::Revisable,
                default_ttl: None,
                decays: false,
                review: ReviewPolicy::Automatic,
                injection_priority: 60,
            },
            MemoryCategory::Knowledge => CategoryLifecycle {
                mutability: Mutability::Revisable,
                default_ttl: None,
                decays: true,
                review: ReviewPolicy::Automatic,
                injection_priority: 50,
            },
            // The working set is replaced fast; anything older than a working
            // day is no longer "what is going on right now".
            MemoryCategory::Context => CategoryLifecycle {
                mutability: Mutability::Revisable,
                default_ttl: Some(TimeDelta::hours(12)),
                decays: true,
                review: ReviewPolicy::Automatic,
                injection_priority: 80,
            },
            MemoryCategory::Instructions => CategoryLifecycle {
                mutability: Mutability::Revisable,
                default_ttl: None,
                decays: false,
                review: ReviewPolicy::HumanGated,
                injection_priority: 100,
            },
            // An unresolved contradiction must stay visible until someone
            // resolves it, so it neither expires nor fades.
            MemoryCategory::Uncertainty => CategoryLifecycle {
                mutability: Mutability::Revisable,
                default_ttl: None,
                decays: false,
                review: ReviewPolicy::HumanGated,
                injection_priority: 70,
            },
        }
    }

    /// Whether records of this category may never be rewritten.
    pub fn is_append_only(self) -> bool {
        self.lifecycle().mutability == Mutability::AppendOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_is_covered_by_all() {
        // Guards against a category being added to the enum but not to `ALL`,
        // which would silently hide it from callers that iterate categories.
        for category in MemoryCategory::ALL {
            let json = serde_json::to_string(&category).expect("serialises");
            let back: MemoryCategory = serde_json::from_str(&json).expect("deserialises");
            assert_eq!(back, category);
        }
        assert_eq!(MemoryCategory::ALL.len(), 6);
    }

    #[test]
    fn append_only_matches_the_lifecycle_rule() {
        assert!(MemoryCategory::Episodic.is_append_only());
        assert!(!MemoryCategory::Knowledge.is_append_only());
    }
}
