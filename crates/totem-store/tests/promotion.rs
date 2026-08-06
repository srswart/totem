//! Promotion: the one sanctioned path across a scope boundary.
//!
//! Every other way a record's scope could change is a leak, so these tests are
//! written the way `scope_isolation.rs` is — against the public API only, with
//! the negative case (who must *not* be able to do this) asserted next to
//! every positive one.

mod common;

use common::{ADA, GRACE, TEAM, chain, chain_with_team, decided_by, memory, repo, store};
use totem_core::{
    ActorId, MemoryCategory, PromotionError, PromotionEventKind, PromotionPolicy, RepoId, Scope,
    ScopeChain, TeamId,
};
use totem_store::{PromotionOutcome, RecallQuery, StoreError};

fn private(id: &str) -> Scope {
    Scope::Actor(ActorId::new(id).expect("valid actor id"))
}

fn shared() -> Scope {
    Scope::Project(repo())
}

fn team_scope() -> Scope {
    Scope::Team(TeamId::new(TEAM).expect("valid team id"))
}

/// A reader working an entirely different repo: their chain reaches neither
/// `project:srswart/totem` nor `team:058-totem`.
fn outsider() -> ScopeChain {
    ScopeChain::resolve(
        &ActorId::new("hopper").expect("valid actor id"),
        Some(&RepoId::new("other/repo").expect("valid repo id")),
        &[],
    )
}

async fn bodies_visible_to(
    store: &totem_store::Store<surrealdb::engine::local::Db>,
    reader: &ScopeChain,
) -> Vec<String> {
    let records = store
        .memories()
        .recall(reader, &RecallQuery::new())
        .await
        .expect("recall succeeds");
    records
        .iter()
        .map(|record| record.content.body.clone())
        .collect()
}

#[tokio::test]
async fn knowledge_proposed_from_a_private_scope_is_promoted_at_once() {
    let store = store().await;
    let note = memory(MemoryCategory::Knowledge, private(ADA), "cosine, 384 dims");
    store
        .memories()
        .save(&chain(ADA), &note)
        .await
        .expect("ada writes her own scope");

    assert!(bodies_visible_to(&store, &chain(GRACE)).await.is_empty());

    let outcome = store
        .promotions()
        .propose(&chain(ADA), note.id, shared(), decided_by(ADA))
        .await
        .expect("knowledge promotes");

    let PromotionOutcome::Promoted { proposal, decision } = outcome else {
        panic!("knowledge should not have queued for a human");
    };
    assert_eq!(decision.kind, PromotionEventKind::AutoApproved);
    assert_eq!(decision.proposal, Some(proposal.id));

    assert_eq!(
        bodies_visible_to(&store, &chain(GRACE)).await,
        vec!["cosine, 384 dims".to_string()],
    );
    let moved = store
        .memories()
        .get(&chain(ADA), note.id)
        .await
        .expect("read succeeds")
        .expect("the record is still there");
    assert_eq!(moved.scope, shared());
}

#[tokio::test]
async fn instructions_wait_in_the_queue_until_a_human_approves() {
    let store = store().await;
    let rule = memory(
        MemoryCategory::Instructions,
        private(ADA),
        "never squash an advance branch",
    );
    store
        .memories()
        .save(&chain(ADA), &rule)
        .await
        .expect("ada writes her own scope");

    let outcome = store
        .promotions()
        .propose(&chain(ADA), rule.id, shared(), decided_by(ADA))
        .await
        .expect("instructions may be proposed");
    let PromotionOutcome::Pending { proposal } = outcome else {
        panic!("instructions must not promote without a human");
    };

    // Queued is not shared: the record has not moved.
    assert!(bodies_visible_to(&store, &chain(GRACE)).await.is_empty());

    let queue = store
        .promotions()
        .pending(&chain(GRACE))
        .await
        .expect("queue reads");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].id, proposal.id);

    store
        .promotions()
        .approve(&chain(GRACE), proposal.id, decided_by(GRACE))
        .await
        .expect("a project member approves");

    assert_eq!(
        bodies_visible_to(&store, &chain(GRACE)).await,
        vec!["never squash an advance branch".to_string()],
    );
    assert!(
        store
            .promotions()
            .pending(&chain(GRACE))
            .await
            .expect("queue reads")
            .is_empty(),
        "a decided proposal must leave the queue",
    );
}

#[tokio::test]
async fn a_rejected_proposal_leaves_the_record_exactly_where_it_was() {
    let store = store().await;
    let rule = memory(MemoryCategory::Instructions, private(ADA), "my own habit");
    store
        .memories()
        .save(&chain(ADA), &rule)
        .await
        .expect("saved");

    let PromotionOutcome::Pending { proposal } = store
        .promotions()
        .propose(&chain(ADA), rule.id, shared(), decided_by(ADA))
        .await
        .expect("proposed")
    else {
        panic!("instructions must queue");
    };

    store
        .promotions()
        .reject(
            &chain(GRACE),
            proposal.id,
            decided_by(GRACE),
            Some("that is a personal preference".to_string()),
        )
        .await
        .expect("a project member rejects");

    assert!(bodies_visible_to(&store, &chain(GRACE)).await.is_empty());
    let unmoved = store
        .memories()
        .get(&chain(ADA), rule.id)
        .await
        .expect("read succeeds")
        .expect("still ada's");
    assert_eq!(unmoved.scope, private(ADA));

    // Rejection is recorded, not merely absent: the audit trail carries both
    // the ask and the refusal.
    let history = store
        .promotions()
        .history(&chain(ADA), rule.id)
        .await
        .expect("history reads");
    let kinds: Vec<PromotionEventKind> = history.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        vec![PromotionEventKind::Proposed, PromotionEventKind::Rejected],
    );
    assert_eq!(
        history[1].reason.as_deref(),
        Some("that is a personal preference"),
    );
}

#[tokio::test]
async fn a_decided_proposal_cannot_be_decided_again() {
    let store = store().await;
    let rule = memory(MemoryCategory::Instructions, private(ADA), "one decision only");
    store
        .memories()
        .save(&chain(ADA), &rule)
        .await
        .expect("saved");
    let PromotionOutcome::Pending { proposal } = store
        .promotions()
        .propose(&chain(ADA), rule.id, shared(), decided_by(ADA))
        .await
        .expect("proposed")
    else {
        panic!("instructions must queue");
    };

    store
        .promotions()
        .approve(&chain(GRACE), proposal.id, decided_by(GRACE))
        .await
        .expect("first decision lands");

    let second = store
        .promotions()
        .reject(&chain(GRACE), proposal.id, decided_by(GRACE), None)
        .await;
    assert!(
        matches!(second, Err(StoreError::PromotionDecided(id)) if id == proposal.id),
        "a second decision must be refused: {second:?}",
    );
}

#[tokio::test]
async fn nobody_can_propose_a_record_they_cannot_see() {
    let store = store().await;
    let note = memory(MemoryCategory::Knowledge, private(ADA), "ada's private note");
    store
        .memories()
        .save(&chain(ADA), &note)
        .await
        .expect("saved");

    let refused = store
        .promotions()
        .propose(&chain(GRACE), note.id, shared(), decided_by(GRACE))
        .await;
    // Absent, not forbidden: the refusal must not confirm the record exists.
    assert!(
        matches!(refused, Err(StoreError::NotFound(id)) if id == note.id),
        "grace promoted a record she cannot read: {refused:?}",
    );
}

#[tokio::test]
async fn nobody_can_propose_into_a_scope_they_cannot_reach() {
    let store = store().await;
    let note = memory(MemoryCategory::Knowledge, private(ADA), "team-only, allegedly");
    store
        .memories()
        .save(&chain(ADA), &note)
        .await
        .expect("saved");

    // ada is not a member of the team, so she cannot deposit memory into it —
    // the same rule `save` already enforces, applied to the target of a
    // promotion.
    let refused = store
        .promotions()
        .propose(&chain(ADA), note.id, team_scope(), decided_by(ADA))
        .await;
    assert!(
        matches!(refused, Err(StoreError::ScopeDenied { scope }) if scope == team_scope()),
        "a non-member promoted into a team scope: {refused:?}",
    );
}

#[tokio::test]
async fn nobody_can_decide_a_proposal_aimed_at_a_scope_they_cannot_reach() {
    let store = store().await;
    let rule = memory(MemoryCategory::Instructions, private(ADA), "team convention");
    store
        .memories()
        .save(&chain_with_team(ADA), &rule)
        .await
        .expect("saved");
    let PromotionOutcome::Pending { proposal } = store
        .promotions()
        .propose(&chain_with_team(ADA), rule.id, team_scope(), decided_by(ADA))
        .await
        .expect("proposed")
    else {
        panic!("instructions must queue");
    };

    let refused = store
        .promotions()
        .approve(&chain(GRACE), proposal.id, decided_by(GRACE))
        .await;
    assert!(
        matches!(refused, Err(StoreError::PromotionNotFound(id)) if id == proposal.id),
        "a non-member approved a promotion into a team: {refused:?}",
    );

    store
        .promotions()
        .approve(&chain_with_team(GRACE), proposal.id, decided_by(GRACE))
        .await
        .expect("a team member may approve");
    let moved = store
        .memories()
        .get(&chain_with_team(ADA), rule.id)
        .await
        .expect("read succeeds")
        .expect("still there");
    assert_eq!(moved.scope, team_scope());
}

#[tokio::test]
async fn episodic_memory_is_refused_before_any_statement_runs() {
    let store = store().await;
    let episode = memory(MemoryCategory::Episodic, private(ADA), "turn 1");
    store
        .memories()
        .save(&chain(ADA), &episode)
        .await
        .expect("saved");

    let refused = store
        .promotions()
        .propose(&chain(ADA), episode.id, shared(), decided_by(ADA))
        .await;
    assert!(
        matches!(
            refused,
            Err(StoreError::Promotion(PromotionError::Forbidden(
                MemoryCategory::Episodic
            )))
        ),
        "an episodic record was proposed for promotion: {refused:?}",
    );
    assert!(
        store
            .promotions()
            .history(&chain(ADA), episode.id)
            .await
            .expect("history reads")
            .is_empty(),
        "a refused promotion must leave no event behind",
    );
}

#[tokio::test]
async fn demotion_compensates_a_promotion() {
    let store = store().await;
    let note = memory(MemoryCategory::Knowledge, private(ADA), "regrettably shared");
    store
        .memories()
        .save(&chain(ADA), &note)
        .await
        .expect("saved");
    store
        .promotions()
        .propose(&chain(ADA), note.id, shared(), decided_by(ADA))
        .await
        .expect("auto-promoted");
    assert_eq!(bodies_visible_to(&store, &chain(GRACE)).await.len(), 1);

    store
        .promotions()
        .demote(
            &chain(ADA),
            note.id,
            private(ADA),
            decided_by(ADA),
            Some("published by mistake".to_string()),
        )
        .await
        .expect("demotion is always available");

    assert!(bodies_visible_to(&store, &chain(GRACE)).await.is_empty());
    let kinds: Vec<PromotionEventKind> = store
        .promotions()
        .history(&chain(ADA), note.id)
        .await
        .expect("history reads")
        .iter()
        .map(|event| event.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            PromotionEventKind::Proposed,
            PromotionEventKind::AutoApproved,
            PromotionEventKind::Demoted,
        ],
    );
}

#[tokio::test]
async fn demotion_cannot_hand_a_record_to_another_actor() {
    let store = store().await;
    let note = memory(MemoryCategory::Knowledge, shared(), "shared project note");
    store
        .memories()
        .save(&chain(ADA), &note)
        .await
        .expect("saved");

    // Narrowing into someone else's private scope would move the record out of
    // everyone's reach and into theirs — a leak wearing a rollback's clothes.
    let refused = store
        .promotions()
        .demote(&chain(ADA), note.id, private(GRACE), decided_by(ADA), None)
        .await;
    assert!(
        matches!(refused, Err(StoreError::ScopeDenied { scope }) if scope == private(GRACE)),
        "ada demoted a shared record into grace's private scope: {refused:?}",
    );
}

#[tokio::test]
async fn the_queue_only_shows_proposals_aimed_at_scopes_the_reader_can_reach() {
    let store = store().await;
    let rule = memory(MemoryCategory::Instructions, private(ADA), "team convention");
    store
        .memories()
        .save(&chain_with_team(ADA), &rule)
        .await
        .expect("saved");
    store
        .promotions()
        .propose(&chain_with_team(ADA), rule.id, team_scope(), decided_by(ADA))
        .await
        .expect("proposed");

    assert!(
        store
            .promotions()
            .pending(&chain(GRACE))
            .await
            .expect("queue reads")
            .is_empty(),
        "a non-member saw a team promotion queue",
    );
    assert_eq!(
        store
            .promotions()
            .pending(&chain_with_team(GRACE))
            .await
            .expect("queue reads")
            .len(),
        1,
    );
}

#[tokio::test]
async fn a_reviewer_reads_the_record_a_pending_proposal_names_and_nothing_else() {
    let store = store().await;
    let rule = memory(MemoryCategory::Instructions, private(ADA), "under review");
    store
        .memories()
        .save(&chain(ADA), &rule)
        .await
        .expect("saved");
    let PromotionOutcome::Pending { proposal } = store
        .promotions()
        .propose(&chain(ADA), rule.id, shared(), decided_by(ADA))
        .await
        .expect("proposed")
    else {
        panic!("instructions must queue");
    };

    // The deliberate disclosure: proposing asks the target scope's reviewers to
    // read this one record, and that is the only thing it opens.
    let under_review = store
        .promotions()
        .proposed_record(&chain(GRACE), proposal.id)
        .await
        .expect("read succeeds")
        .expect("a reviewer sees what they are deciding on");
    assert_eq!(under_review.content.body, "under review");

    // Ordinary recall still cannot reach it — the record has not moved.
    assert!(
        store
            .memories()
            .get(&chain(GRACE), rule.id)
            .await
            .expect("read succeeds")
            .is_none(),
        "a pending proposal moved the record early",
    );
    // And nobody outside the target scope gets the disclosure at all.
    assert!(
        store
            .promotions()
            .proposed_record(&outsider(), proposal.id)
            .await
            .expect("read succeeds")
            .is_none(),
        "an outsider read a record under review in a scope they cannot reach",
    );

    store
        .promotions()
        .reject(&chain(GRACE), proposal.id, decided_by(GRACE), None)
        .await
        .expect("rejected");
    assert!(
        store
            .promotions()
            .proposed_record(&chain(GRACE), proposal.id)
            .await
            .expect("read succeeds")
            .is_none(),
        "a decided proposal must stop disclosing the record",
    );
}

#[tokio::test]
async fn the_history_of_a_record_is_scope_filtered() {
    let store = store().await;
    let note = memory(MemoryCategory::Knowledge, private(ADA), "ada's own");
    store
        .memories()
        .save(&chain(ADA), &note)
        .await
        .expect("saved");
    store
        .promotions()
        .propose(&chain(ADA), note.id, shared(), decided_by(ADA))
        .await
        .expect("auto-promoted");

    // grace shares the project the record landed in, so the promotion is hers
    // to audit; hopper shares neither end of it and sees nothing.
    assert_eq!(
        store
            .promotions()
            .history(&chain(GRACE), note.id)
            .await
            .expect("history reads")
            .len(),
        2,
    );
    assert!(
        store
            .promotions()
            .history(&outsider(), note.id)
            .await
            .expect("history reads")
            .is_empty(),
        "an outsider read another project's promotion history",
    );
}

#[tokio::test]
async fn the_tightened_policy_queues_even_a_knowledge_promotion() {
    // Risk + Rollback's other lever: policy can be tightened to
    // human-gated-for-everything without touching a category definition.
    let store = store().await;
    let note = memory(MemoryCategory::Knowledge, private(ADA), "normally automatic");
    store
        .memories()
        .save(&chain(ADA), &note)
        .await
        .expect("saved");

    let outcome = store
        .promotions_with_policy(PromotionPolicy::human_gated_everywhere())
        .propose(&chain(ADA), note.id, shared(), decided_by(ADA))
        .await
        .expect("proposed");
    assert!(
        matches!(outcome, PromotionOutcome::Pending { .. }),
        "the tightened policy let a knowledge promotion through",
    );
    assert!(bodies_visible_to(&store, &chain(GRACE)).await.is_empty());
}

#[tokio::test]
async fn every_recorded_event_names_who_decided_it_and_when() {
    // "Recorded events with full provenance" is the objective's own wording:
    // an event that could not answer "who moved this, from where, via which
    // surface" would make the scope change unauditable.
    let store = store().await;
    let rule = memory(MemoryCategory::Instructions, private(ADA), "attributable");
    store
        .memories()
        .save(&chain(ADA), &rule)
        .await
        .expect("saved");
    let PromotionOutcome::Pending { proposal } = store
        .promotions()
        .propose(&chain(ADA), rule.id, shared(), decided_by(ADA))
        .await
        .expect("proposed")
    else {
        panic!("instructions must queue");
    };
    store
        .promotions()
        .approve(&chain(GRACE), proposal.id, decided_by(GRACE))
        .await
        .expect("approved");

    let history = store
        .promotions()
        .history(&chain(ADA), rule.id)
        .await
        .expect("history reads");
    assert_eq!(history.len(), 2);
    // The proposer and the approver are recorded separately: an approval that
    // inherited the proposer's identity would make self-approval invisible.
    assert_eq!(history[0].provenance.author.actor().to_string(), ADA);
    assert_eq!(history[1].provenance.author.actor().to_string(), GRACE);
    for event in &history {
        assert_eq!(event.provenance.harness, totem_core::Harness::Console);
        assert_eq!(event.memory, rule.id);
        assert_eq!(event.from_scope, private(ADA));
        assert_eq!(event.to_scope, shared());
    }
}
