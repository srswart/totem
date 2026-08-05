//! Category lifecycle, provenance, and the common memory record shape
//! (docs/solution-intent.md §2.1).

use chrono::{DateTime, TimeDelta, Utc};
use totem_core::{
    ActorId, Author, Content, Harness, LifecycleError, MemoryCategory, MemoryRecord, MemoryStatus,
    Mutability, Provenance, RepoId, ReviewPolicy, ReviewState, Scope, SessionId, SubjectKind,
    SubjectRef,
};

fn created_at() -> DateTime<Utc> {
    "2026-08-05T06:00:00Z".parse().expect("valid timestamp")
}

fn provenance() -> Provenance {
    Provenance::new(
        Author::Agent(ActorId::new("claude-code").expect("valid actor id")),
        Harness::ClaudeCode,
        SessionId::new("sess-1").expect("valid session id"),
        created_at(),
    )
}

fn project_scope() -> Scope {
    Scope::Project(RepoId::new("srswart/totem").expect("valid repo id"))
}

fn record(category: MemoryCategory) -> MemoryRecord {
    MemoryRecord::new(
        category,
        project_scope(),
        Content::new("the store enforces scope isolation"),
        provenance(),
    )
}

#[test]
fn there_are_exactly_six_memory_categories() {
    assert_eq!(
        MemoryCategory::ALL,
        [
            MemoryCategory::Episodic,
            MemoryCategory::Identity,
            MemoryCategory::Knowledge,
            MemoryCategory::Context,
            MemoryCategory::Instructions,
            MemoryCategory::Uncertainty,
        ]
    );
}

#[test]
fn categories_serialise_as_stable_snake_case_names() {
    for category in MemoryCategory::ALL {
        let json = serde_json::to_string(&category).expect("serialises");
        let back: MemoryCategory = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back, category);
    }

    assert_eq!(
        serde_json::to_string(&MemoryCategory::Instructions).expect("serialises"),
        "\"instructions\""
    );
}

#[test]
fn only_episodic_memory_is_append_only() {
    for category in MemoryCategory::ALL {
        let expected = if category == MemoryCategory::Episodic {
            Mutability::AppendOnly
        } else {
            Mutability::Revisable
        };
        assert_eq!(category.lifecycle().mutability, expected, "{category:?}");
    }
}

#[test]
fn only_context_memory_carries_a_default_ttl() {
    assert_eq!(
        MemoryCategory::Context.lifecycle().default_ttl,
        Some(TimeDelta::hours(12))
    );

    for category in MemoryCategory::ALL {
        if category != MemoryCategory::Context {
            assert_eq!(category.lifecycle().default_ttl, None, "{category:?}");
        }
    }
}

#[test]
fn knowledge_decays_without_reinforcement_and_the_audit_substrate_does_not() {
    assert!(MemoryCategory::Knowledge.lifecycle().decays);
    assert!(!MemoryCategory::Episodic.lifecycle().decays);
    assert!(!MemoryCategory::Instructions.lifecycle().decays);
    assert!(!MemoryCategory::Uncertainty.lifecycle().decays);
}

#[test]
fn instructions_are_human_gated_and_injected_first() {
    assert_eq!(
        MemoryCategory::Instructions.lifecycle().review,
        ReviewPolicy::HumanGated
    );
    assert_eq!(
        MemoryCategory::Knowledge.lifecycle().review,
        ReviewPolicy::Automatic
    );

    let instructions = MemoryCategory::Instructions.lifecycle().injection_priority;
    for category in MemoryCategory::ALL {
        if category != MemoryCategory::Instructions {
            assert!(
                instructions > category.lifecycle().injection_priority,
                "instructions must outrank {category:?}"
            );
        }
    }
}

#[test]
fn a_new_record_carries_its_provenance_verbatim() {
    let record = record(MemoryCategory::Knowledge);

    assert_eq!(record.provenance.created_at, created_at());
    assert_eq!(record.provenance.harness, Harness::ClaudeCode);
    assert_eq!(
        record.provenance.session,
        SessionId::new("sess-1").expect("valid session id")
    );
    assert_eq!(record.provenance.turn, None);
    assert!(record.provenance.derived_from.is_empty());
    assert_eq!(
        record.provenance.author.actor(),
        &ActorId::new("claude-code").expect("valid actor id")
    );
}

#[test]
fn derived_records_link_back_to_the_episodes_they_came_from() {
    let episode = record(MemoryCategory::Episodic);
    let derived = MemoryRecord::new(
        MemoryCategory::Knowledge,
        project_scope(),
        Content::new("distilled from the episode"),
        provenance().derived_from(vec![episode.id]),
    );

    assert_eq!(derived.provenance.derived_from, vec![episode.id]);
}

#[test]
fn a_new_record_starts_with_fresh_economics_and_active_governance() {
    let record = record(MemoryCategory::Knowledge);

    assert_eq!(record.economics.use_count, 0);
    assert_eq!(record.economics.last_used_at, None);
    assert_eq!(record.governance.status, MemoryStatus::Active);
    assert_eq!(record.governance.review, ReviewState::NotRequired);
}

#[test]
fn a_new_instructions_record_starts_pending_human_review() {
    assert_eq!(
        record(MemoryCategory::Instructions).governance.review,
        ReviewState::Pending
    );
}

#[test]
fn revising_an_episodic_record_is_refused() {
    let mut episode = record(MemoryCategory::Episodic);
    let original = episode.content.body.clone();

    let result = episode.revise(Content::new("rewritten history"));

    assert_eq!(
        result,
        Err(LifecycleError::AppendOnly(MemoryCategory::Episodic))
    );
    assert_eq!(episode.content.body, original);
}

#[test]
fn revising_a_revisable_record_replaces_its_content() {
    let mut knowledge = record(MemoryCategory::Knowledge);

    knowledge
        .revise(Content::new("refined fact").with_tags(["scope"]))
        .expect("knowledge is revisable");

    assert_eq!(knowledge.content.body, "refined fact");
    assert_eq!(knowledge.content.tags, vec!["scope".to_string()]);
}

#[test]
fn expiry_follows_the_category_ttl() {
    assert_eq!(
        record(MemoryCategory::Context).expires_at(),
        Some(created_at() + TimeDelta::hours(12))
    );
    assert_eq!(record(MemoryCategory::Knowledge).expires_at(), None);
}

#[test]
fn a_record_can_point_at_the_arrive_artifact_it_concerns() {
    let mut record = record(MemoryCategory::Context);
    record.subject =
        Some(SubjectRef::new(SubjectKind::Advance, "ADV-CORE-001").expect("valid ref"));

    let subject = record.subject.as_ref().expect("subject set");
    assert_eq!(subject.kind, SubjectKind::Advance);
    assert_eq!(subject.id, "ADV-CORE-001");
    assert!(SubjectRef::new(SubjectKind::Advance, "").is_err());
}

#[test]
fn records_round_trip_through_json() {
    let mut original = record(MemoryCategory::Context);
    original.content.embedding = Some(vec![0.5, -0.25]);
    original.content.tags = vec!["scope".to_string()];
    original.subject = Some(SubjectRef::new(SubjectKind::Component, "core").expect("valid ref"));

    let json = serde_json::to_string(&original).expect("serialises");
    let back: MemoryRecord = serde_json::from_str(&json).expect("deserialises");

    assert_eq!(back, original);
}
