//! Promotion policy: which scope changes may happen at all, and who has to
//! agree before one does.
//!
//! Promotion is the single sanctioned path across a scope boundary
//! (docs/solution-intent.md §2.2), so the rules that decide a path are worth
//! testing on their own, away from any store. What this file asserts is the
//! *policy*; that the store actually obeys it is `totem-store`'s own
//! `tests/promotion.rs`.

use totem_core::{
    ActorId, Author, Harness, MemoryCategory, MemoryId, PromotionError, PromotionEvent,
    PromotionEventKind, PromotionPath, PromotionPolicy, Provenance, RepoId, ReviewPolicy, Scope,
    SessionId, TeamId,
};

fn actor(id: &str) -> ActorId {
    ActorId::new(id).expect("valid actor id")
}

fn private() -> Scope {
    Scope::Actor(actor("ada"))
}

fn project() -> Scope {
    Scope::Project(RepoId::new("srswart/totem").expect("valid repo id"))
}

fn team() -> Scope {
    Scope::Team(TeamId::new("058-totem").expect("valid team id"))
}

fn provenance() -> Provenance {
    Provenance::new(
        Author::Human(actor("ada")),
        Harness::Console,
        SessionId::new("sess-1").expect("valid session id"),
        "2026-08-06T06:00:00Z".parse().expect("valid timestamp"),
    )
}

#[test]
fn knowledge_promotes_automatically_and_instructions_wait_for_a_human() {
    // The rule the objective names outright: auto-approval for low-risk
    // categories, a human gate for standing rules.
    let policy = PromotionPolicy::new();
    assert_eq!(
        policy
            .check_promotion(MemoryCategory::Knowledge, &private(), &project())
            .expect("knowledge may be promoted"),
        PromotionPath::Automatic,
    );
    assert_eq!(
        policy
            .check_promotion(MemoryCategory::Instructions, &private(), &project())
            .expect("instructions may be promoted"),
        PromotionPath::HumanGated,
    );
}

#[test]
fn every_category_takes_the_path_its_own_review_policy_already_implies() {
    // One table, not two: the promotion gate is the category's existing
    // ReviewPolicy, so a category cannot be human-gated for review and quietly
    // automatic for sharing.
    let policy = PromotionPolicy::new();
    for category in MemoryCategory::ALL {
        let expected = if category.is_append_only() {
            PromotionPath::Forbidden
        } else {
            match category.lifecycle().review {
                ReviewPolicy::Automatic => PromotionPath::Automatic,
                ReviewPolicy::HumanGated => PromotionPath::HumanGated,
            }
        };
        assert_eq!(policy.path(category), expected, "{category:?}");
    }
}

#[test]
fn episodic_memory_can_never_change_scope_in_either_direction() {
    // An episodic row cannot be touched at all (the schema's own EVENT refuses
    // UPDATE), so a scope change is not merely gated — it is impossible, and
    // the policy says so before any statement is built.
    let policy = PromotionPolicy::new();
    assert_eq!(
        policy.path(MemoryCategory::Episodic),
        PromotionPath::Forbidden
    );
    assert_eq!(
        policy.check_promotion(MemoryCategory::Episodic, &private(), &project()),
        Err(PromotionError::Forbidden(MemoryCategory::Episodic)),
    );
    assert_eq!(
        policy.check_demotion(MemoryCategory::Episodic, &project(), &private()),
        Err(PromotionError::Forbidden(MemoryCategory::Episodic)),
    );
}

#[test]
fn promotion_must_strictly_widen() {
    let policy = PromotionPolicy::new();
    assert!(
        policy
            .check_promotion(MemoryCategory::Knowledge, &private(), &Scope::Platform)
            .is_ok()
    );
    assert_eq!(
        policy.check_promotion(MemoryCategory::Knowledge, &project(), &private()),
        Err(PromotionError::NotWidening {
            from: project(),
            to: private(),
        }),
    );
    // Same scope is not a promotion either: a no-op that recorded an approval
    // would put an untrue event in the audit trail.
    assert_eq!(
        policy.check_promotion(MemoryCategory::Knowledge, &project(), &project()),
        Err(PromotionError::NotWidening {
            from: project(),
            to: project(),
        }),
    );
}

#[test]
fn demotion_must_strictly_narrow() {
    let policy = PromotionPolicy::new();
    assert!(
        policy
            .check_demotion(MemoryCategory::Instructions, &Scope::Platform, &team())
            .is_ok()
    );
    assert_eq!(
        policy.check_demotion(MemoryCategory::Knowledge, &project(), &Scope::Platform),
        Err(PromotionError::NotNarrowing {
            from: project(),
            to: Scope::Platform,
        }),
    );
}

#[test]
fn demotion_is_never_gated_because_narrowing_is_the_rollback() {
    // Risk + Rollback promises demotion compensates a bad promotion. A demotion
    // that itself had to queue for approval would not be a rollback lever.
    let policy = PromotionPolicy::new();
    for category in MemoryCategory::ALL
        .into_iter()
        .filter(|c| !c.is_append_only())
    {
        assert!(
            policy
                .check_demotion(category, &Scope::Platform, &private())
                .is_ok(),
            "{category:?}",
        );
    }
}

#[test]
fn the_tightened_policy_gates_every_category_a_human_could_gate() {
    // The documented rollback: tighten to human-gated-for-everything. Episodic
    // stays forbidden — tightening never loosens.
    let policy = PromotionPolicy::human_gated_everywhere();
    assert_eq!(
        policy.path(MemoryCategory::Knowledge),
        PromotionPath::HumanGated
    );
    assert_eq!(
        policy.path(MemoryCategory::Identity),
        PromotionPath::HumanGated
    );
    assert_eq!(
        policy.path(MemoryCategory::Episodic),
        PromotionPath::Forbidden
    );
}

#[test]
fn a_decision_inherits_the_proposal_it_answers() {
    let memory = MemoryId::new();
    let proposal = PromotionEvent::propose(memory, private(), project(), provenance());
    assert_eq!(proposal.kind, PromotionEventKind::Proposed);
    assert_eq!(proposal.proposal, None);

    let approval = proposal.approved(provenance());
    assert_eq!(approval.kind, PromotionEventKind::Approved);
    // The decision cannot name a different record or a different target than
    // the proposal it answers — there is no constructor that would let it.
    assert_eq!(approval.proposal, Some(proposal.id));
    assert_eq!(approval.memory, proposal.memory);
    assert_eq!(approval.from_scope, proposal.from_scope);
    assert_eq!(approval.to_scope, proposal.to_scope);
    assert_ne!(approval.id, proposal.id);
}

#[test]
fn only_the_events_that_move_a_record_say_they_take_effect() {
    let memory = MemoryId::new();
    let proposal = PromotionEvent::propose(memory, private(), project(), provenance());

    assert!(!proposal.takes_effect());
    assert!(!proposal.rejected(provenance()).takes_effect());
    assert!(proposal.approved(provenance()).takes_effect());
    assert!(proposal.auto_approved(provenance()).takes_effect());
    assert!(PromotionEvent::demotion(memory, project(), private(), provenance()).takes_effect());
}

#[test]
fn a_reason_is_optional_and_kept_verbatim() {
    let proposal = PromotionEvent::propose(MemoryId::new(), private(), project(), provenance());
    assert_eq!(proposal.reason, None);
    assert_eq!(
        proposal
            .rejected(provenance())
            .with_reason("belongs in the runbook, not project memory")
            .reason
            .as_deref(),
        Some("belongs in the runbook, not project memory"),
    );
}
