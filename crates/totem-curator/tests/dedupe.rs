//! The dedupe job end to end (ADV-CURATOR-001).
//!
//! Every test builds its own embedded `kv-mem` store and seeds its own records
//! (docs/tech-direction/surrealdb.md §4): nothing here assumes a running
//! `surreal` server, a port, or a container. Embeddings are constructed by hand
//! rather than produced by an embedder, so what a test asserts about similarity
//! is a property of the vectors it wrote, not of a model's behaviour.

use chrono::{DateTime, Utc};
use surrealdb::engine::local::Db;
use totem_core::{
    AccessOperation, ActorId, Author, Content, Harness, MemoryCategory, MemoryRecord, MemoryStatus,
    Provenance, RepoId, Scope, ScopeChain, SessionId, SubjectKind, SubjectRef,
};
use totem_curator::{Curator, DedupePolicy};
use totem_store::{EMBEDDING_DIMENSIONS, RecallQuery, Store};

const ADA: &str = "ada";
const REPO: &str = "srswart/totem";

async fn store() -> Store<Db> {
    let store = Store::in_memory().await.expect("embedded engine connects");
    store.migrate().await.expect("migrations apply");
    store
}

fn actor(id: &str) -> ActorId {
    ActorId::new(id).expect("valid actor id")
}

fn chain(id: &str) -> ScopeChain {
    ScopeChain::resolve(
        &actor(id),
        Some(&RepoId::new(REPO).expect("valid repo id")),
        &[],
    )
}

fn project() -> Scope {
    Scope::Project(RepoId::new(REPO).expect("valid repo id"))
}

fn at(timestamp: &str) -> DateTime<Utc> {
    timestamp.parse().expect("valid RFC 3339 timestamp")
}

fn curator(store: &Store<Db>) -> Curator<'_, Db> {
    Curator::new(
        store,
        actor("totem-curator"),
        SessionId::new("curate-1").expect("valid session id"),
    )
}

/// A vector that is `tilt` away from the unit vector on `axis`.
///
/// `tilt` 0.0 is the axis itself; larger tilts rotate towards a second axis, so
/// a test can place two records at a chosen cosine similarity without depending
/// on any embedder's behaviour.
fn tilted(axis: usize, tilt: f32) -> Vec<f32> {
    let mut embedding = vec![0.0; EMBEDDING_DIMENSIONS];
    embedding[axis % EMBEDDING_DIMENSIONS] = 1.0;
    embedding[(axis + 1) % EMBEDDING_DIMENSIONS] = tilt;
    embedding
}

struct Written {
    scope: Scope,
    body: String,
    written_at: String,
    embedding: Option<Vec<f32>>,
    subject: Option<SubjectRef>,
}

fn note(body: &str) -> Written {
    Written {
        scope: project(),
        body: body.to_string(),
        written_at: "2026-08-05T06:00:00Z".to_string(),
        embedding: Some(tilted(1, 0.0)),
        subject: None,
    }
}

impl Written {
    fn at_scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self
    }

    fn written_at(mut self, timestamp: &str) -> Self {
        self.written_at = timestamp.to_string();
        self
    }

    fn embedded(mut self, embedding: Option<Vec<f32>>) -> Self {
        self.embedding = embedding;
        self
    }

    fn about(mut self, id: &str) -> Self {
        self.subject = Some(SubjectRef::new(SubjectKind::Component, id).expect("valid subject"));
        self
    }

    async fn save(self, store: &Store<Db>, writer: &ScopeChain) -> MemoryRecord {
        let mut content = Content::new(self.body);
        content.embedding = self.embedding;
        let mut record = MemoryRecord::new(
            MemoryCategory::Knowledge,
            self.scope,
            content,
            Provenance::new(
                Author::Agent(actor(ADA)),
                Harness::ClaudeCode,
                SessionId::new("sess-1").expect("valid session id"),
                at(&self.written_at),
            ),
        );
        record.subject = self.subject;
        store
            .memories()
            .save(writer, &record)
            .await
            .expect("the record saves");
        record
    }
}

async fn status_of(store: &Store<Db>, reader: &ScopeChain, id: totem_core::MemoryId) -> MemoryStatus {
    store
        .memories()
        .get(reader, id)
        .await
        .expect("the read succeeds")
        .expect("the record is still there")
        .governance
        .status
}

#[tokio::test]
async fn near_duplicates_become_one_survivor_that_cites_them() {
    let store = store().await;
    let ada = chain(ADA);
    let older = note("deploys happen on fridays")
        .written_at("2026-08-05T06:00:00Z")
        .save(&store, &ada)
        .await;
    let newer = note("deploys happen on fridays, after standup")
        .written_at("2026-08-05T09:00:00Z")
        .embedded(Some(tilted(1, 0.01)))
        .save(&store, &ada)
        .await;

    let report = curator(&store)
        .dedupe(&ada, &DedupePolicy::new())
        .await
        .expect("the job runs");

    assert_eq!(report.merges.len(), 1, "{report:?}");
    assert_eq!(report.examined, 2);
    let event = &report.merges[0];
    let mut cited = event.superseded_ids();
    cited.sort();
    let mut expected = vec![older.id, newer.id];
    expected.sort();
    assert_eq!(cited, expected);

    let survivor = store
        .memories()
        .get(&ada, event.merged)
        .await
        .expect("the read succeeds")
        .expect("the survivor exists");
    // The newest member's wording is what the survivor keeps: a merge that
    // preferred the older text would quietly undo the most recent correction.
    assert_eq!(survivor.content.body, newer.content.body);
    assert_eq!(survivor.provenance.derived_from.len(), 2);
    assert!(
        matches!(survivor.provenance.author, Author::Curator(_)),
        "the survivor was not attributed to the curator: {:?}",
        survivor.provenance.author,
    );
    assert_eq!(status_of(&store, &ada, older.id).await, MemoryStatus::Retired);
    assert_eq!(status_of(&store, &ada, newer.id).await, MemoryStatus::Retired);
}

#[tokio::test]
async fn a_merely_related_record_is_left_alone() {
    let store = store().await;
    let ada = chain(ADA);
    note("deploys happen on fridays").save(&store, &ada).await;
    let unrelated = note("the store enforces scope isolation")
        .embedded(Some(tilted(7, 0.0)))
        .save(&store, &ada)
        .await;

    let report = curator(&store)
        .dedupe(&ada, &DedupePolicy::new())
        .await
        .expect("the job runs");

    assert!(report.merges.is_empty(), "{report:?}");
    assert_eq!(
        status_of(&store, &ada, unrelated.id).await,
        MemoryStatus::Active
    );
}

#[tokio::test]
async fn a_second_run_finds_nothing_left_to_do() {
    let store = store().await;
    let ada = chain(ADA);
    note("deploys happen on fridays").save(&store, &ada).await;
    note("deploys happen on fridays, after standup")
        .embedded(Some(tilted(1, 0.01)))
        .save(&store, &ada)
        .await;

    let first = curator(&store)
        .dedupe(&ada, &DedupePolicy::new())
        .await
        .expect("the first run works");
    let second = curator(&store)
        .dedupe(&ada, &DedupePolicy::new())
        .await
        .expect("the second run works");

    assert_eq!(first.merges.len(), 1);
    assert!(
        second.merges.is_empty(),
        "the job merged its own output: {second:?}",
    );
    assert_eq!(
        store
            .memories()
            .recall(&ada, &RecallQuery::new())
            .await
            .expect("recall succeeds")
            .len(),
        1,
    );
}

#[tokio::test]
async fn identical_records_at_different_scopes_are_never_merged() {
    // Leak bait: the same sentence held privately and at project scope. A merge
    // would either publish the private copy or pull the shared one into one
    // actor's scope; neither is something a curator may decide.
    let store = store().await;
    let ada = chain(ADA);
    let mine = note("deploys happen on fridays")
        .at_scope(Scope::Actor(actor(ADA)))
        .save(&store, &ada)
        .await;
    let ours = note("deploys happen on fridays")
        .save(&store, &ada)
        .await;

    let report = curator(&store)
        .dedupe(&ada, &DedupePolicy::new())
        .await
        .expect("the job runs");

    assert!(
        report.merges.is_empty(),
        "a curator merged across a scope boundary: {report:?}",
    );
    assert_eq!(status_of(&store, &ada, mine.id).await, MemoryStatus::Active);
    assert_eq!(status_of(&store, &ada, ours.id).await, MemoryStatus::Active);
}

#[tokio::test]
async fn records_about_different_subjects_are_not_merged() {
    // Graph context, not vector similarity alone: two records that read alike
    // but concern different components are two facts, not one duplicate.
    let store = store().await;
    let ada = chain(ADA);
    note("the component is incubating")
        .about("store")
        .save(&store, &ada)
        .await;
    note("the component is incubating")
        .about("gateway")
        .save(&store, &ada)
        .await;

    let report = curator(&store)
        .dedupe(&ada, &DedupePolicy::new())
        .await
        .expect("the job runs");

    assert!(report.merges.is_empty(), "{report:?}");
}

#[tokio::test]
async fn a_record_without_an_embedding_is_reported_rather_than_guessed_at() {
    let store = store().await;
    let ada = chain(ADA);
    note("deploys happen on fridays")
        .embedded(None)
        .save(&store, &ada)
        .await;
    note("deploys happen on fridays")
        .embedded(None)
        .save(&store, &ada)
        .await;

    let report = curator(&store)
        .dedupe(&ada, &DedupePolicy::new())
        .await
        .expect("the job runs");

    assert!(report.merges.is_empty(), "{report:?}");
    assert_eq!(report.skipped_without_embedding, 2);
}

#[tokio::test]
async fn every_curator_action_appends_to_the_access_log() {
    let store = store().await;
    let ada = chain(ADA);
    note("deploys happen on fridays").save(&store, &ada).await;
    note("deploys happen on fridays, after standup")
        .embedded(Some(tilted(1, 0.01)))
        .save(&store, &ada)
        .await;

    let report = curator(&store)
        .dedupe(&ada, &DedupePolicy::new())
        .await
        .expect("the job runs");
    curator(&store)
        .rollback(&ada, report.merges[0].id, None)
        .await
        .expect("the rollback applies");

    let entries = store
        .access_log()
        .list()
        .await
        .expect("the access log reads");
    let curator_entries: Vec<_> = entries
        .iter()
        .filter(|entry| entry.harness == Harness::Curator)
        .collect();
    let endpoints: Vec<&str> = curator_entries
        .iter()
        .map(|entry| entry.endpoint.as_str())
        .collect();
    assert_eq!(
        endpoints,
        vec![
            "/curator/dedupe/scan",
            "/curator/dedupe/merge",
            "/curator/dedupe/rollback"
        ],
        "a curator action touched memory without an audit entry",
    );
    assert!(
        curator_entries
            .iter()
            .all(|entry| entry.actor.to_string() == "totem-curator"),
    );
    assert_eq!(curator_entries[0].operation, AccessOperation::Recall);
    assert_eq!(curator_entries[0].result_count, Some(2));
    assert_eq!(curator_entries[1].operation, AccessOperation::Save);
    assert_eq!(curator_entries[1].memory_id, Some(report.merges[0].merged));
}

#[tokio::test]
async fn a_rollback_through_the_runner_restores_what_the_job_superseded() {
    let store = store().await;
    let ada = chain(ADA);
    let older = note("deploys happen on fridays").save(&store, &ada).await;
    let newer = note("deploys happen on fridays, after standup")
        .embedded(Some(tilted(1, 0.01)))
        .save(&store, &ada)
        .await;
    let report = curator(&store)
        .dedupe(&ada, &DedupePolicy::new())
        .await
        .expect("the job runs");
    let merge = &report.merges[0];

    let rollback = curator(&store)
        .rollback(&ada, merge.id, Some("not the same fact".to_string()))
        .await
        .expect("the rollback applies");

    assert_eq!(rollback.rolls_back, Some(merge.id));
    assert_eq!(status_of(&store, &ada, older.id).await, MemoryStatus::Active);
    assert_eq!(status_of(&store, &ada, newer.id).await, MemoryStatus::Active);
    assert_eq!(
        status_of(&store, &ada, merge.merged).await,
        MemoryStatus::Retired
    );
}

#[tokio::test]
async fn a_stricter_threshold_refuses_a_merge_the_default_would_make() {
    // The threshold is the job's one tuning knob, and it is a real one: the
    // same pair merges under the default and does not under a stricter policy.
    let store = store().await;
    let ada = chain(ADA);
    note("deploys happen on fridays").save(&store, &ada).await;
    note("deploys happen on fridays, after standup")
        .embedded(Some(tilted(1, 0.2)))
        .save(&store, &ada)
        .await;

    let strict = curator(&store)
        .dedupe(&ada, &DedupePolicy::new().with_threshold(0.999))
        .await
        .expect("the job runs");
    assert!(strict.merges.is_empty(), "{strict:?}");

    let lenient = curator(&store)
        .dedupe(&ada, &DedupePolicy::new().with_threshold(0.9))
        .await
        .expect("the job runs");
    assert_eq!(lenient.merges.len(), 1, "{lenient:?}");
}
