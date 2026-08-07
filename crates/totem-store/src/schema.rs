//! The SurrealDB schema and the constants downstream advances are pinned to.
//!
//! Every constraint here derives from an executed finding, not a preference:
//! see `docs/tech-direction/surrealdb.md` (TD-002..TD-011) and
//! `docs/tech-direction/embeddings.md` (EMB-004).

/// The vector dimension every stored embedding must have.
///
/// Fixed now, before any embedding exists, so that ADV-STORE-002's pipeline
/// lands against an index it does not have to migrate: EMB-004 measured
/// BGE-small-en-v1.5 at 384 dimensions with cosine distance
/// (docs/tech-direction/embeddings.md). Changing it re-opens every stored
/// vector, so it is asserted against the live index definition in this module's
/// tests.
pub const EMBEDDING_DIMENSIONS: usize = 384;

/// The ledger of applied migrations.
///
/// Defined outside the migration list because it must exist before the list can
/// be consulted, and it is therefore the one piece of DDL that runs on every
/// connection.
pub(crate) const MIGRATION_LEDGER: &str = r#"
DEFINE TABLE IF NOT EXISTS schema_migration SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS version ON schema_migration TYPE int;
DEFINE FIELD IF NOT EXISTS name ON schema_migration TYPE string;
DEFINE FIELD IF NOT EXISTS applied_at ON schema_migration TYPE datetime;
"#;

/// Migration 1 — typed memory, the landscape graph, and the invariants.
///
/// Three things here are enforcement rather than description:
///
/// - `provenance` and its members are required, so an unattributable row is
///   refused by the database rather than by a code path someone can forget.
/// - `embedding` is `array<float, 384>`, so a wrong-dimension vector is refused
///   before it can corrupt the HNSW index.
/// - the two `EVENT`s refuse `UPDATE` and `DELETE` on episodic rows. This is
///   deliberately stricter than "the repository refuses to revise": it also
///   binds curator jobs, backfills, and future statements written inside this
///   crate.
///
/// Note what the append-only event costs: an episodic row cannot be touched at
/// all, so per-record retrieval counters are impossible on episodic memory.
/// Metering episodic reads belongs in the access log (ADV-GATEWAY-001), not in
/// `economics.use_count`.
///
/// No index is defined on `scope`. The scope predicate is pushed into the
/// `KnnScan` as it stands (TD-003), and a second candidate index is exactly the
/// kind of change that makes the planner choose a different operator — which
/// would silently retire the plan assertion in this module's sibling tests.
/// Revisit under ADV-GATEWAY-007, with the plan assertion updated in the same
/// change.
pub(crate) const MEMORY_SCHEMA_V1: &str = r#"
DEFINE TABLE memory SCHEMAFULL;

DEFINE FIELD category ON memory TYPE string
    ASSERT $value IN ['episodic', 'identity', 'knowledge', 'context', 'instructions', 'uncertainty'];
DEFINE FIELD scope ON memory TYPE string ASSERT $value != '';
DEFINE FIELD subject ON memory
    TYPE option<record<repo | system | component | advance | actor | memory>>;
DEFINE FIELD body ON memory TYPE string;
DEFINE FIELD embedding ON memory TYPE option<array<float, 384>>;
DEFINE FIELD tags ON memory TYPE array<string> DEFAULT [];

DEFINE FIELD provenance ON memory TYPE object;
DEFINE FIELD provenance.author_kind ON memory TYPE string
    ASSERT $value IN ['human', 'agent', 'curator'];
DEFINE FIELD provenance.author ON memory TYPE string ASSERT $value != '';
DEFINE FIELD provenance.harness ON memory TYPE string ASSERT $value != '';
DEFINE FIELD provenance.session ON memory TYPE string ASSERT $value != '';
DEFINE FIELD provenance.turn ON memory TYPE option<int>;
DEFINE FIELD provenance.created_at ON memory TYPE datetime;
DEFINE FIELD provenance.derived_from ON memory TYPE array<record<memory>> DEFAULT [];

DEFINE FIELD economics ON memory TYPE object;
DEFINE FIELD economics.use_count ON memory TYPE int DEFAULT 0;
DEFINE FIELD economics.last_used_at ON memory TYPE option<datetime>;
DEFINE FIELD economics.value_score ON memory TYPE float DEFAULT 1.0;
DEFINE FIELD economics.currency ON memory TYPE float DEFAULT 1.0;

DEFINE FIELD governance ON memory TYPE object;
DEFINE FIELD governance.status ON memory TYPE string
    ASSERT $value IN ['active', 'contested', 'retired'];
DEFINE FIELD governance.review ON memory TYPE string
    ASSERT $value IN ['not_required', 'pending', 'approved', 'rejected'];

DEFINE INDEX memory_embedding ON memory FIELDS embedding HNSW DIMENSION 384 DIST COSINE;

DEFINE EVENT memory_episodic_no_update ON TABLE memory
    WHEN $event = 'UPDATE' AND $before.category = 'episodic'
    THEN { THROW 'episodic memory is append-only and cannot be updated'; };
DEFINE EVENT memory_episodic_no_delete ON TABLE memory
    WHEN $event = 'DELETE' AND $before.category = 'episodic'
    THEN { THROW 'episodic memory is append-only and cannot be deleted'; };

DEFINE TABLE repo SCHEMAFULL;
DEFINE FIELD name ON repo TYPE string;

DEFINE TABLE system SCHEMAFULL;
DEFINE FIELD name ON system TYPE string;
DEFINE FIELD repo ON system TYPE option<record<repo>>;

DEFINE TABLE component SCHEMAFULL;
DEFINE FIELD name ON component TYPE string;
DEFINE FIELD stage ON component TYPE option<string>;
DEFINE FIELD system ON component TYPE option<record<system>>;

DEFINE TABLE advance SCHEMAFULL;
DEFINE FIELD title ON advance TYPE string;
DEFINE FIELD status ON advance TYPE option<string>;
DEFINE FIELD system ON advance TYPE option<record<system>>;

DEFINE TABLE actor SCHEMAFULL;
DEFINE FIELD name ON actor TYPE string;

DEFINE TABLE impacts SCHEMALESS TYPE RELATION IN advance OUT component;
DEFINE TABLE depends_on SCHEMALESS TYPE RELATION IN component OUT component;
DEFINE TABLE owned_by SCHEMALESS TYPE RELATION IN component OUT actor;
"#;

/// Migration 2 — the access log (ADV-GATEWAY-001).
///
/// Every read and write appends one row here (docs/project-brief.md G3). Like
/// episodic memory, an audit trail that could be rewritten would not be an
/// audit trail: the two `EVENT`s below refuse `UPDATE` and `DELETE` at the
/// database level, so no future code path — not a curator job, not a
/// "harmless" backfill — can alter or remove an access record.
pub(crate) const ACCESS_LOG_SCHEMA_V2: &str = r#"
DEFINE TABLE access_log SCHEMAFULL;

DEFINE FIELD actor ON access_log TYPE string ASSERT $value != '';
DEFINE FIELD harness ON access_log TYPE string ASSERT $value != '';
DEFINE FIELD session ON access_log TYPE string ASSERT $value != '';
DEFINE FIELD turn ON access_log TYPE option<int>;
DEFINE FIELD operation ON access_log TYPE string ASSERT $value IN ['recall', 'save'];
DEFINE FIELD endpoint ON access_log TYPE string ASSERT $value != '';
DEFINE FIELD memory_id ON access_log TYPE option<record<memory>>;
DEFINE FIELD result_count ON access_log TYPE option<int>;
DEFINE FIELD at ON access_log TYPE datetime;

DEFINE EVENT access_log_no_update ON TABLE access_log
    WHEN $event = 'UPDATE'
    THEN { THROW 'the access log is append-only and cannot be updated'; };
DEFINE EVENT access_log_no_delete ON TABLE access_log
    WHEN $event = 'DELETE'
    THEN { THROW 'the access log is append-only and cannot be deleted'; };
"#;

/// Migration 3 — landscape sync support (ADV-ARRIVE-SYNC-001).
///
/// Two additions, both needed before ingestion can write anything real:
///
/// - `component_id` recovers a component's short artifact id from its
///   system-namespaced record key (`<system>__<component>`, chosen so two
///   systems may each declare a component of the same short name without
///   colliding).
/// - `sync_run` is the provenance record every ingestion run writes — the
///   same append-only pattern as `access_log`, because a sync's provenance
///   must be as durable as the audit trail it feeds
///   (`arrive-sync.yaml`: "Every ingestion records sync provenance").
pub(crate) const LANDSCAPE_SYNC_SCHEMA_V3: &str = r#"
DEFINE FIELD component_id ON component TYPE string;

DEFINE TABLE sync_run SCHEMAFULL;

DEFINE FIELD repo ON sync_run TYPE record<repo>;
DEFINE FIELD source ON sync_run TYPE string ASSERT $value != '';
DEFINE FIELD started_at ON sync_run TYPE datetime;
DEFINE FIELD completed_at ON sync_run TYPE datetime;
DEFINE FIELD systems_synced ON sync_run TYPE int;
DEFINE FIELD components_synced ON sync_run TYPE int;
DEFINE FIELD advances_synced ON sync_run TYPE int;

DEFINE EVENT sync_run_no_update ON TABLE sync_run
    WHEN $event = 'UPDATE'
    THEN { THROW 'sync provenance is append-only and cannot be updated'; };
DEFINE EVENT sync_run_no_delete ON TABLE sync_run
    WHEN $event = 'DELETE'
    THEN { THROW 'sync provenance is append-only and cannot be deleted'; };
"#;

/// Migration 4 — the `feedback` access-log operation (ADV-GATEWAY-004
/// gap-fill).
///
/// `OVERWRITE` widens `operation`'s `ASSERT` set rather than replacing the
/// field definition wholesale: existing rows (already one of `recall`/`save`)
/// are unaffected, and only the assertion a new `feedback` row must satisfy
/// changes. Plain `DEFINE FIELD` refuses to redefine a field that already
/// exists (unlike `DEFINE TABLE`/`DEFINE FIELD IF NOT EXISTS` on the
/// migration ledger, which only ever needs to *create*), so `OVERWRITE` is
/// required here, not optional.
pub(crate) const ACCESS_LOG_FEEDBACK_SCHEMA_V4: &str = r#"
DEFINE FIELD OVERWRITE operation ON access_log TYPE string
    ASSERT $value IN ['recall', 'save', 'feedback'];
"#;

/// Migration 5 — recorded scope promotions (ADV-CORE-003).
///
/// Promotion is the one sanctioned path across a scope boundary
/// (docs/solution-intent.md §2.2), so its trail is held to the same standard as
/// the access log: the two `EVENT`s refuse `UPDATE` and `DELETE`, and
/// `provenance` is a required object with required members. An event that could
/// be rewritten, removed, or written anonymously would make the project's
/// highest-severity operation unauditable.
///
/// `recorded_at` is assigned by the store, not by the caller. `provenance.
/// created_at` is whatever the calling harness reported, so ordering the trail
/// by it would let a caller with a wrong — or deliberately backdated — clock
/// reorder the record of who decided what, and when.
pub(crate) const PROMOTION_SCHEMA_V5: &str = r#"
DEFINE TABLE promotion_event SCHEMAFULL;

DEFINE FIELD memory ON promotion_event TYPE record<memory>;
DEFINE FIELD kind ON promotion_event TYPE string
    ASSERT $value IN ['proposed', 'auto_approved', 'approved', 'rejected', 'demoted'];
DEFINE FIELD from_scope ON promotion_event TYPE string ASSERT $value != '';
DEFINE FIELD to_scope ON promotion_event TYPE string ASSERT $value != '';
DEFINE FIELD proposal ON promotion_event TYPE option<record<promotion_event>>;
DEFINE FIELD reason ON promotion_event TYPE option<string>;
DEFINE FIELD recorded_at ON promotion_event TYPE datetime;

DEFINE FIELD provenance ON promotion_event TYPE object;
DEFINE FIELD provenance.author_kind ON promotion_event TYPE string
    ASSERT $value IN ['human', 'agent', 'curator'];
DEFINE FIELD provenance.author ON promotion_event TYPE string ASSERT $value != '';
DEFINE FIELD provenance.harness ON promotion_event TYPE string ASSERT $value != '';
DEFINE FIELD provenance.session ON promotion_event TYPE string ASSERT $value != '';
DEFINE FIELD provenance.turn ON promotion_event TYPE option<int>;
DEFINE FIELD provenance.created_at ON promotion_event TYPE datetime;
DEFINE FIELD provenance.derived_from ON promotion_event TYPE array<record<memory>> DEFAULT [];

DEFINE EVENT promotion_event_no_update ON TABLE promotion_event
    WHEN $event = 'UPDATE'
    THEN { THROW 'a promotion event is append-only and cannot be updated'; };
DEFINE EVENT promotion_event_no_delete ON TABLE promotion_event
    WHEN $event = 'DELETE'
    THEN { THROW 'a promotion event is append-only and cannot be deleted'; };
"#;

/// Migration 6 — recorded curator actions (ADV-CURATOR-001).
///
/// The curator is the first writer that touches records it did not author, and
/// its whole safety argument is that every action is reversible. That argument
/// rests on this table, so it is held to the same standard as the access log
/// and the promotion trail: the two `EVENT`s refuse `UPDATE` and `DELETE`, and
/// `provenance` is a required object with required members. A rollback is
/// reconstructed from the event alone — including `superseded.*.prior_status`,
/// the status each original held *before* the merge — so an event that could be
/// rewritten or removed would leave a merge with nothing to undo it by.
///
/// `scope` is the one scope every record in the event sits at: a merge may not
/// cross a scope boundary (`totem_core::CurationPolicy`), which is what lets
/// the trail be scope-filtered on a single column rather than by joining back
/// to each record.
///
/// `recorded_at` is assigned by the store for the same reason it is on
/// `promotion_event`: `provenance.created_at` is whatever the calling job
/// reported, and ordering an audit trail by a caller's clock lets a wrong one
/// rearrange history.
pub(crate) const CURATION_SCHEMA_V6: &str = r#"
DEFINE TABLE curation_event SCHEMAFULL;

DEFINE FIELD kind ON curation_event TYPE string
    ASSERT $value IN ['merged', 'rolled_back'];
DEFINE FIELD merged ON curation_event TYPE record<memory>;
DEFINE FIELD scope ON curation_event TYPE string ASSERT $value != '';
DEFINE FIELD superseded ON curation_event TYPE array<object>;
DEFINE FIELD superseded.*.memory ON curation_event TYPE record<memory>;
DEFINE FIELD superseded.*.prior_status ON curation_event TYPE string
    ASSERT $value IN ['active', 'contested', 'retired'];
DEFINE FIELD rolls_back ON curation_event TYPE option<record<curation_event>>;
DEFINE FIELD reason ON curation_event TYPE option<string>;
DEFINE FIELD recorded_at ON curation_event TYPE datetime;

DEFINE FIELD provenance ON curation_event TYPE object;
DEFINE FIELD provenance.author_kind ON curation_event TYPE string
    ASSERT $value IN ['human', 'agent', 'curator'];
DEFINE FIELD provenance.author ON curation_event TYPE string ASSERT $value != '';
DEFINE FIELD provenance.harness ON curation_event TYPE string ASSERT $value != '';
DEFINE FIELD provenance.session ON curation_event TYPE string ASSERT $value != '';
DEFINE FIELD provenance.turn ON curation_event TYPE option<int>;
DEFINE FIELD provenance.created_at ON curation_event TYPE datetime;
DEFINE FIELD provenance.derived_from ON curation_event TYPE array<record<memory>> DEFAULT [];

DEFINE EVENT curation_event_no_update ON TABLE curation_event
    WHEN $event = 'UPDATE'
    THEN { THROW 'a curation event is append-only and cannot be updated'; };
DEFINE EVENT curation_event_no_delete ON TABLE curation_event
    WHEN $event = 'DELETE'
    THEN { THROW 'a curation event is append-only and cannot be deleted'; };
"#;

/// Migration 7 — the `propose`, `promotion_decision`, and `resolve`
/// access-log operations (ADV-CONSOLE-002).
///
/// Same `OVERWRITE` technique migration 4 established: existing rows are
/// unaffected, and only the assertion a new row must satisfy widens.
pub(crate) const ACCESS_LOG_GOVERNANCE_SCHEMA_V7: &str = r#"
DEFINE FIELD OVERWRITE operation ON access_log TYPE string
    ASSERT $value IN ['recall', 'save', 'feedback', 'propose', 'promotion_decision', 'resolve'];
"#;

/// Migration 8 — the repo's `owner/name` GitHub identity (ADV-GATEWAY-009).
///
/// `repo` was keyed by the ARRIVE registry id alone (`registry.repo_id`,
/// e.g. `"058-totem"`); a gateway credential's binding speaks a different id
/// space (`owner/name`, e.g. `"srswart/totem"`). `option<string>` because a
/// row synced before this migration has no value until its next sync
/// converges it (`LandscapeRepository::sync`'s `UPSERT CONTENT` always
/// writes it going forward) — no backfill statement, so a row nobody
/// re-syncs is simply not yet confirmed, not corrupted.
pub(crate) const REPO_GIT_IDENTITY_SCHEMA_V8: &str = r#"
DEFINE FIELD git_repo ON repo TYPE option<string>;
"#;

#[cfg(test)]
mod tests {
    //! Enforcement the repository API cannot be trusted to provide.
    //!
    //! These live inside the crate on purpose: they issue statements straight
    //! at the connection, which is exactly what an integration test *cannot*
    //! do. If the database refuses them, then no future code path in this
    //! crate — a curator job, a backfill, a "harmless" migration — can rewrite
    //! the audit substrate or store an unattributable row either.

    use surrealdb::types::Value;

    use crate::{EMBEDDING_DIMENSIONS, Store};

    async fn migrated() -> Store<surrealdb::engine::local::Db> {
        let store = Store::in_memory().await.expect("embedded engine connects");
        store.migrate().await.expect("migrations apply");
        store
    }

    const EPISODE: &str = r#"
        CREATE memory:episode CONTENT {
            category: 'episodic', scope: 'project:srswart/totem', body: 'turn 1',
            tags: [],
            provenance: { author_kind: 'agent', author: 'ada', harness: 'claude_code',
                          session: 's1', created_at: d'2026-08-05T06:00:00Z', derived_from: [] },
            economics: { use_count: 0, value_score: 1.0, currency: 1.0 },
            governance: { status: 'active', review: 'not_required' }
        }
    "#;

    async fn body_of(store: &Store<surrealdb::engine::local::Db>, thing: &str) -> String {
        let mut response = store
            .connection()
            .query(format!("SELECT VALUE body FROM {thing}"))
            .await
            .expect("select sent")
            .check()
            .expect("select succeeded");
        let rows: Value = response.take(0).expect("rows");
        let rows = rows.into_array().expect("an array");
        rows.iter()
            .next()
            .and_then(|row| row.clone().into_string().ok())
            .expect("the record exists")
    }

    #[tokio::test]
    async fn the_database_refuses_to_update_an_episodic_row() {
        let store = migrated().await;
        store
            .connection()
            .query(EPISODE)
            .await
            .expect("sent")
            .check()
            .expect("the episode is written");

        let refused = store
            .connection()
            .query("UPDATE memory:episode SET body = 'a tidier account'")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw UPDATE rewrote an episodic record: {refused:?}",
        );
        assert_eq!(body_of(&store, "memory:episode").await, "turn 1");
    }

    #[tokio::test]
    async fn the_database_refuses_to_delete_an_episodic_row() {
        let store = migrated().await;
        store
            .connection()
            .query(EPISODE)
            .await
            .expect("sent")
            .check()
            .expect("the episode is written");

        let refused = store
            .connection()
            .query("DELETE memory:episode")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw DELETE removed an episodic record: {refused:?}",
        );
        assert_eq!(body_of(&store, "memory:episode").await, "turn 1");
    }

    #[tokio::test]
    async fn the_database_refuses_a_row_without_provenance() {
        let store = migrated().await;
        let refused = store
            .connection()
            .query(
                "CREATE memory:unattributed CONTENT {
                    category: 'knowledge', scope: 'project:srswart/totem',
                    body: 'who wrote this?', tags: [],
                    economics: { use_count: 0, value_score: 1.0, currency: 1.0 },
                    governance: { status: 'active', review: 'not_required' }
                }",
            )
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "an unattributable row was accepted: {refused:?}",
        );
    }

    #[tokio::test]
    async fn a_revisable_row_is_still_updatable() {
        // The negative control: if the append-only event refused *everything*,
        // the two tests above would pass against a store that cannot be
        // written to at all.
        let store = migrated().await;
        store
            .connection()
            .query(
                EPISODE
                    .replace("memory:episode", "memory:note")
                    .replace("'episodic'", "'knowledge'"),
            )
            .await
            .expect("sent")
            .check()
            .expect("the note is written");

        store
            .connection()
            .query("UPDATE memory:note SET body = 'revised'")
            .await
            .expect("sent")
            .check()
            .expect("knowledge is revisable");
        assert_eq!(body_of(&store, "memory:note").await, "revised");
    }

    const ACCESS_LOG_ENTRY: &str = r#"
        CREATE access_log CONTENT {
            actor: 'ada', harness: 'claude_code', session: 's1',
            operation: 'recall', endpoint: '/recall', result_count: 3,
            at: d'2026-08-05T06:00:00Z'
        }
    "#;

    #[tokio::test]
    async fn the_database_refuses_to_update_an_access_log_row() {
        let store = migrated().await;
        let mut response = store
            .connection()
            .query(ACCESS_LOG_ENTRY)
            .await
            .expect("sent")
            .check()
            .expect("the entry is written");
        let created: Value = response.take(0).expect("created row");
        let id = created
            .into_array()
            .expect("an array")
            .into_iter()
            .next()
            .and_then(|row| row.into_object().ok())
            .and_then(|row| row.get("id").cloned())
            .expect("the created row has an id");

        let refused = store
            .connection()
            .query("UPDATE $id SET endpoint = '/tampered'")
            .bind(("id", id))
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw UPDATE rewrote an access log entry: {refused:?}",
        );
    }

    #[tokio::test]
    async fn the_database_refuses_to_delete_an_access_log_row() {
        let store = migrated().await;
        store
            .connection()
            .query(ACCESS_LOG_ENTRY)
            .await
            .expect("sent")
            .check()
            .expect("the entry is written");

        let refused = store
            .connection()
            .query("DELETE access_log")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw DELETE removed an access log entry: {refused:?}",
        );

        let mut response = store
            .connection()
            .query("SELECT VALUE endpoint FROM access_log")
            .await
            .expect("sent")
            .check()
            .expect("select succeeded");
        let rows: Value = response.take(0).expect("rows");
        assert_eq!(rows.into_array().expect("an array").len(), 1);
    }

    const SYNC_RUN_ENTRY: &str = r#"
        CREATE sync_run CONTENT {
            repo: repo:test, source: 'test', started_at: d'2026-08-05T06:00:00Z',
            completed_at: d'2026-08-05T06:00:01Z', systems_synced: 1,
            components_synced: 1, advances_synced: 1
        }
    "#;

    #[tokio::test]
    async fn the_database_refuses_to_update_a_sync_run_row() {
        let store = migrated().await;
        store
            .connection()
            .query(SYNC_RUN_ENTRY)
            .await
            .expect("sent")
            .check()
            .expect("the run is written");

        let refused = store
            .connection()
            .query("UPDATE sync_run SET source = 'tampered'")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw UPDATE rewrote a sync_run entry: {refused:?}",
        );
    }

    #[tokio::test]
    async fn the_database_refuses_to_delete_a_sync_run_row() {
        let store = migrated().await;
        store
            .connection()
            .query(SYNC_RUN_ENTRY)
            .await
            .expect("sent")
            .check()
            .expect("the run is written");

        let refused = store
            .connection()
            .query("DELETE sync_run")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw DELETE removed a sync_run entry: {refused:?}",
        );

        let mut response = store
            .connection()
            .query("SELECT VALUE source FROM sync_run")
            .await
            .expect("sent")
            .check()
            .expect("select succeeded");
        let rows: Value = response.take(0).expect("rows");
        assert_eq!(rows.into_array().expect("an array").len(), 1);
    }

    const PROMOTION_EVENT: &str = r#"
        CREATE promotion_event:proposal CONTENT {
            memory: memory:episode, kind: 'proposed',
            from_scope: 'actor:ada', to_scope: 'project:srswart/totem',
            recorded_at: d'2026-08-06T06:00:00Z',
            provenance: { author_kind: 'human', author: 'ada', harness: 'console',
                          session: 's1', created_at: d'2026-08-06T06:00:00Z', derived_from: [] }
        }
    "#;

    #[tokio::test]
    async fn the_database_refuses_to_update_a_promotion_event() {
        // A promotion event is the record that a scope boundary was crossed.
        // If it could be rewritten, the audit trail of the project's
        // highest-severity operation would be worth nothing.
        let store = migrated().await;
        store
            .connection()
            .query(PROMOTION_EVENT)
            .await
            .expect("sent")
            .check()
            .expect("the event is written");

        let refused = store
            .connection()
            .query("UPDATE promotion_event:proposal SET kind = 'approved'")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw UPDATE rewrote a promotion event: {refused:?}",
        );
    }

    #[tokio::test]
    async fn the_database_refuses_to_delete_a_promotion_event() {
        let store = migrated().await;
        store
            .connection()
            .query(PROMOTION_EVENT)
            .await
            .expect("sent")
            .check()
            .expect("the event is written");

        let refused = store
            .connection()
            .query("DELETE promotion_event")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw DELETE removed a promotion event: {refused:?}",
        );

        let mut response = store
            .connection()
            .query("SELECT VALUE kind FROM promotion_event")
            .await
            .expect("sent")
            .check()
            .expect("select succeeded");
        let rows: Value = response.take(0).expect("rows");
        assert_eq!(rows.into_array().expect("an array").len(), 1);
    }

    #[tokio::test]
    async fn the_database_refuses_a_promotion_event_without_provenance() {
        let store = migrated().await;
        let refused = store
            .connection()
            .query(
                "CREATE promotion_event CONTENT {
                    memory: memory:episode, kind: 'proposed',
                    from_scope: 'actor:ada', to_scope: 'project:srswart/totem',
                    recorded_at: d'2026-08-06T06:00:00Z'
                }",
            )
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "an unattributable promotion event was accepted: {refused:?}",
        );
    }

    #[tokio::test]
    async fn the_database_refuses_a_scope_edit_on_an_episodic_row() {
        // Defence in depth behind PromotionPolicy: even if the policy were
        // loosened by mistake, the append-only EVENT still refuses to move an
        // episodic record.
        let store = migrated().await;
        store
            .connection()
            .query(EPISODE)
            .await
            .expect("sent")
            .check()
            .expect("the episode is written");

        let refused = store
            .connection()
            .query("UPDATE memory:episode SET scope = 'platform'")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw UPDATE promoted an episodic record: {refused:?}",
        );
    }

    const CURATION_EVENT: &str = r#"
        CREATE curation_event:merge CONTENT {
            kind: 'merged', merged: memory:survivor, scope: 'project:srswart/totem',
            superseded: [{ memory: memory:first, prior_status: 'active' }],
            recorded_at: d'2026-08-06T08:00:00Z',
            provenance: { author_kind: 'curator', author: 'totem-curator', harness: 'curator',
                          session: 'curate-1', created_at: d'2026-08-06T08:00:00Z', derived_from: [] }
        }
    "#;

    #[tokio::test]
    async fn the_database_refuses_to_update_a_curation_event() {
        // A curation event is the record that a curator retired somebody's
        // memory, and the only thing a rollback can be reconstructed from. If
        // it could be rewritten, "reversible" would be a claim with nothing
        // behind it.
        let store = migrated().await;
        store
            .connection()
            .query(CURATION_EVENT)
            .await
            .expect("sent")
            .check()
            .expect("the event is written");

        let refused = store
            .connection()
            .query("UPDATE curation_event:merge SET kind = 'rolled_back'")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw UPDATE rewrote a curation event: {refused:?}",
        );
    }

    #[tokio::test]
    async fn the_database_refuses_to_delete_a_curation_event() {
        let store = migrated().await;
        store
            .connection()
            .query(CURATION_EVENT)
            .await
            .expect("sent")
            .check()
            .expect("the event is written");

        let refused = store
            .connection()
            .query("DELETE curation_event")
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "a raw DELETE removed a curation event: {refused:?}",
        );

        let mut response = store
            .connection()
            .query("SELECT VALUE kind FROM curation_event")
            .await
            .expect("sent")
            .check()
            .expect("select succeeded");
        let rows: Value = response.take(0).expect("rows");
        assert_eq!(rows.into_array().expect("an array").len(), 1);
    }

    #[tokio::test]
    async fn the_database_refuses_a_curation_event_without_provenance() {
        let store = migrated().await;
        let refused = store
            .connection()
            .query(
                "CREATE curation_event CONTENT {
                    kind: 'merged', merged: memory:survivor, scope: 'project:srswart/totem',
                    superseded: [{ memory: memory:first, prior_status: 'active' }],
                    recorded_at: d'2026-08-06T08:00:00Z'
                }",
            )
            .await
            .expect("sent")
            .check();
        assert!(
            refused.is_err(),
            "an unattributable curation event was accepted: {refused:?}",
        );
    }

    #[tokio::test]
    async fn the_vector_index_is_pinned_to_the_measured_dimension_and_distance() {
        let store = migrated().await;
        let mut response = store
            .connection()
            .query("INFO FOR TABLE memory")
            .await
            .expect("sent")
            .check()
            .expect("info succeeded");
        let info: Value = response.take(0).expect("info");
        let info = format!("{info:?}");

        assert!(
            info.contains(&format!(
                "HNSW DIMENSION {EMBEDDING_DIMENSIONS} DIST COSINE"
            )),
            "the vector index drifted from the EMB-004 pin: {info}",
        );
    }
}
