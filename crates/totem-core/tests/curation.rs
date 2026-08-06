//! What a curator may and may not do to memory it did not write
//! (ADV-CURATOR-001).
//!
//! The component invariant is one sentence — "curation never deletes; originals
//! are superseded, every action is logged and reversible"
//! (`components/curator.yaml`) — and this file is the domain half of it. The
//! store half (that a merge really does retire exactly the rows it claims, and
//! that a rollback really does put them back) lives in
//! `totem-store/tests/curation.rs`; nothing here touches a database.

use chrono::Utc;
use totem_core::{
    ActorId, Author, Content, CurationError, CurationEventKind, CurationPolicy, Harness,
    MemoryCategory, MemoryRecord, MemoryStatus, Provenance, RepoId, Scope, SessionId,
};

fn curator() -> Provenance {
    Provenance::new(
        Author::Curator(ActorId::new("totem-curator").expect("valid actor id")),
        Harness::Curator,
        SessionId::new("curate-1").expect("valid session id"),
        Utc::now(),
    )
}

fn project() -> Scope {
    Scope::Project(RepoId::new("srswart/totem").expect("valid repo id"))
}

fn private() -> Scope {
    Scope::Actor(ActorId::new("ada").expect("valid actor id"))
}

fn record(category: MemoryCategory, scope: Scope, body: &str) -> MemoryRecord {
    MemoryRecord::new(category, scope, Content::new(body), curator())
}

fn knowledge(scope: Scope, body: &str) -> MemoryRecord {
    record(MemoryCategory::Knowledge, scope, body)
}

#[test]
fn a_merge_records_every_original_it_supersedes_with_the_status_it_had() {
    let first = knowledge(project(), "the store enforces scope isolation");
    let mut second = knowledge(project(), "scope isolation is enforced in the store");
    second.governance.status = MemoryStatus::Contested;
    let merged = knowledge(project(), "scope isolation is enforced in the store");

    let event = CurationPolicy::new()
        .merge(&merged, &[first.clone(), second.clone()], curator())
        .expect("a same-scope knowledge merge is allowed");

    assert_eq!(event.kind, CurationEventKind::Merged);
    assert_eq!(event.merged, merged.id);
    assert_eq!(event.scope, project());
    assert_eq!(event.superseded_ids(), vec![first.id, second.id]);
    // The prior status travels with the event because rollback restores it:
    // an event that only remembered "these were superseded" could only ever
    // put records back as `active`, quietly promoting a contested record.
    assert_eq!(event.superseded[0].prior_status, MemoryStatus::Active);
    assert_eq!(event.superseded[1].prior_status, MemoryStatus::Contested);
    assert_eq!(event.rolls_back, None);
}

#[test]
fn a_merge_never_crosses_a_scope_boundary() {
    // The project's highest-severity failure, in curator form: merging a
    // private note into a project record would publish it to everyone on the
    // repo, with no promotion event and nobody's approval.
    let mine = knowledge(private(), "we deploy on fridays");
    let ours = knowledge(project(), "we deploy on fridays");
    let merged = knowledge(project(), "we deploy on fridays");

    let refused = CurationPolicy::new().merge(&merged, &[ours, mine.clone()], curator());

    assert_eq!(
        refused.expect_err("a cross-scope merge must be refused"),
        CurationError::CrossScope {
            merged: project(),
            superseded: private(),
        }
    );
}

#[test]
fn only_categories_a_curator_may_act_on_alone_can_be_merged() {
    // ReviewPolicy::Automatic is documented as "no human gate; curators may act
    // on it directly" — so it, and nothing else, is what a curator may merge.
    // Instructions and Uncertainty are human-gated, and episodic memory is the
    // append-only audit substrate.
    for category in MemoryCategory::ALL {
        let first = record(category, project(), "a duplicate");
        let second = record(category, project(), "a duplicate");
        let merged = record(category, project(), "a duplicate");

        let outcome = CurationPolicy::new().merge(&merged, &[first, second], curator());
        let allowed = outcome.is_ok();

        assert_eq!(
            allowed,
            CurationPolicy::new().may_curate(category),
            "{category:?} disagreed with the policy it publishes",
        );
        if !allowed {
            assert_eq!(
                outcome.expect_err("refused"),
                CurationError::Forbidden(category)
            );
        }
    }
}

#[test]
fn a_merge_needs_at_least_two_originals_and_none_of_them_may_be_itself() {
    let policy = CurationPolicy::new();
    let merged = knowledge(project(), "a fact");
    let other = knowledge(project(), "a fact");

    assert_eq!(
        policy
            .merge(&merged, std::slice::from_ref(&other), curator())
            .expect_err("one original is a rename, not a merge"),
        CurationError::NothingToMerge { superseded: 1 }
    );
    assert_eq!(
        policy
            .merge(&merged, &[], curator())
            .expect_err("no originals is not a merge at all"),
        CurationError::NothingToMerge { superseded: 0 }
    );
    assert_eq!(
        policy
            .merge(&merged, &[other, merged.clone()], curator())
            .expect_err("a record cannot supersede itself"),
        CurationError::SelfSupersede(merged.id)
    );
}

#[test]
fn a_merge_does_not_mix_categories() {
    let policy = CurationPolicy::new();
    let knowledge_record = knowledge(project(), "a fact");
    let context_record = record(MemoryCategory::Context, project(), "a fact");
    let merged = knowledge(project(), "a fact");

    assert_eq!(
        policy
            .merge(
                &merged,
                &[knowledge_record, context_record.clone()],
                curator()
            )
            .expect_err("categories decide lifecycle, so a merged pair must share one"),
        CurationError::MixedCategory {
            merged: MemoryCategory::Knowledge,
            superseded: MemoryCategory::Context,
        }
    );
}

#[test]
fn a_retired_record_is_not_superseded_a_second_time() {
    // Idempotency starts here: a job that re-reads its own output must not
    // merge what a previous run already retired.
    let policy = CurationPolicy::new();
    let mut already = knowledge(project(), "a fact");
    already.governance.status = MemoryStatus::Retired;
    let live = knowledge(project(), "a fact");
    let merged = knowledge(project(), "a fact");

    assert_eq!(
        policy
            .merge(&merged, &[live, already.clone()], curator())
            .expect_err("a retired record has already been superseded"),
        CurationError::NotActive(already.id)
    );
}

#[test]
fn a_rollback_answers_exactly_one_merge_and_carries_the_same_originals() {
    let first = knowledge(project(), "a fact");
    let second = knowledge(project(), "a fact, restated");
    let merged = knowledge(project(), "a fact, restated");
    let event = CurationPolicy::new()
        .merge(&merged, &[first.clone(), second.clone()], curator())
        .expect("merge allowed");

    let rollback = event
        .rolled_back(curator())
        .expect("a merge can be rolled back")
        .with_reason("the two facts were not the same fact");

    assert_eq!(rollback.kind, CurationEventKind::RolledBack);
    assert_eq!(rollback.rolls_back, Some(event.id));
    assert_eq!(rollback.merged, merged.id);
    assert_eq!(rollback.scope, project());
    assert_eq!(rollback.superseded, event.superseded);
    assert_eq!(
        rollback.reason.as_deref(),
        Some("the two facts were not the same fact")
    );
    assert_ne!(rollback.id, event.id);

    // Rolling back a rollback would put a second, contradictory restoration in
    // the trail; the compensating action for "I restored these" is another
    // merge, which is a new decision with its own event.
    assert_eq!(
        rollback
            .rolled_back(curator())
            .expect_err("a rollback is not itself rollbackable"),
        CurationError::NotAMerge(rollback.id)
    );
}
