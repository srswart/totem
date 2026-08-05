//! ADV-STORE-004 — investigation spike for SurrealDB's multi-model claims.
//!
//! This crate is **investigation evidence, not production code**. It exists to
//! answer one load-bearing question before `totem-store` commits to a schema
//! (docs/solution-intent.md §1): can a single SurrealQL round trip assemble
//! graph traversal + vector search + temporal facts into complete context, and
//! do a turn's writes commit as one ACID transaction? Findings are written up
//! in `docs/tech-direction/surrealdb.md`; `totem-store` (ADV-STORE-001) is
//! expected to derive its own schema rather than import this one.
//!
//! The experiments live here rather than in the test files so that the *same*
//! assertions can run against two engines — the embedded `kv-mem` engine every
//! test uses, and a real `surreal start` server — which is how the advance's
//! engine-parity question gets answered. See `tests/embedded.rs` (runs by
//! default) and `tests/server_parity.rs` (opt-in: `server-parity` feature plus
//! `TOTEM_SPIKE_SURREAL_URL`).

use std::time::Duration;

use futures::StreamExt;
use surrealdb::types::{Action, Datetime, Object, Value};
use surrealdb::{Connection, Surreal};

/// Toy schema: a document table with a vector index, a graph edge between
/// memories, and a record link to an ARRIVE-ish component entity. Small on
/// purpose — the smallest shape that exercises all four models at once.
pub const TOY_SCHEMA: &str = r#"
DEFINE TABLE component SCHEMAFULL;
DEFINE FIELD name ON component TYPE string;

DEFINE TABLE memory SCHEMAFULL;
DEFINE FIELD category ON memory TYPE string;
DEFINE FIELD scope ON memory TYPE string;
DEFINE FIELD body ON memory TYPE string;
DEFINE FIELD embedding ON memory TYPE array<float>;
DEFINE FIELD created_at ON memory TYPE datetime;
DEFINE FIELD uses ON memory TYPE int DEFAULT 0;
DEFINE FIELD subject ON memory TYPE option<record<component>>;
DEFINE INDEX mem_embedding ON memory FIELDS embedding HNSW DIMENSION 4 DIST COSINE;

DEFINE TABLE event SCHEMAFULL;
DEFINE FIELD kind ON event TYPE string;

DEFINE TABLE derived_from SCHEMALESS TYPE RELATION IN memory OUT memory;
"#;

/// Seed data. `mine_*` records sit in a project scope the caller may read;
/// `foreign_*` records sit in another actor's scope and must never come back.
/// The foreign records are deliberately *closer* to the probe vector than the
/// readable ones, so a query that truncated to top-K before applying the scope
/// filter would visibly under-recall.
pub const TOY_SEED: &str = r#"
CREATE component:store SET name = 'Totem Store';

CREATE memory:mine_rule SET category = 'knowledge', scope = 'project:srswart/totem',
    body = 'scope isolation is enforced in the store', embedding = [0.0, 1.0, 0.0, 0.0],
    created_at = d'2026-08-01T00:00:00Z', subject = component:store;
CREATE memory:mine_episode SET category = 'episodic', scope = 'project:srswart/totem',
    body = 'the turn that produced the rule', embedding = [0.0, 0.9, 0.1, 0.0],
    created_at = d'2026-07-01T00:00:00Z', subject = component:store;
CREATE memory:mine_stale SET category = 'context', scope = 'project:srswart/totem',
    body = 'working set from months ago', embedding = [0.0, 1.0, 0.0, 0.0],
    created_at = d'2026-01-01T00:00:00Z', subject = component:store;

CREATE memory:foreign_a SET category = 'knowledge', scope = 'actor:someone-else',
    body = 'private to another actor', embedding = [1.0, 0.0, 0.0, 0.0],
    created_at = d'2026-08-01T00:00:00Z', subject = component:store;
CREATE memory:foreign_b SET category = 'knowledge', scope = 'actor:someone-else',
    body = 'also private', embedding = [1.0, 0.0, 0.0, 0.0],
    created_at = d'2026-08-01T00:00:00Z', subject = component:store;

RELATE memory:mine_rule->derived_from->memory:mine_episode;
"#;

/// The read half of read → think → write, as one statement: HNSW vector search
/// (`<|K,EF|>`), graph traversal out along `derived_from`, a record-link hop to
/// the subject entity, a temporal cutoff, and the scope filter — all resolved
/// before a single row leaves the database.
pub const RECALL_QUERY: &str = r#"
SELECT
    id,
    category,
    body,
    vector::distance::knn() AS distance,
    subject.name AS subject_name,
    ->derived_from->memory.body AS derived_from_bodies
FROM memory
WHERE embedding <|3,40|> $probe
  AND scope IN $scopes
  AND created_at > $since
ORDER BY distance ASC
"#;

/// The write half: a decision, an entity update, and a triggered event, in one
/// transaction.
pub const TURN_WRITE: &str = r#"
BEGIN;
CREATE memory:decision SET category = 'knowledge', scope = 'project:srswart/totem',
    body = 'one round trip is enough', embedding = [0.0, 1.0, 0.0, 0.0],
    created_at = d'2026-08-05T00:00:00Z', subject = component:store;
UPDATE memory:mine_rule SET uses += 1;
CREATE event:promotion SET kind = 'promotion';
COMMIT;
"#;

/// The same turn, but the event write violates the schema. Nothing from this
/// transaction — not even the two valid statements before it — may survive.
pub const TURN_WRITE_FAILING: &str = r#"
BEGIN;
CREATE memory:orphan SET category = 'knowledge', scope = 'project:srswart/totem',
    body = 'must not survive', embedding = [0.0, 1.0, 0.0, 0.0],
    created_at = d'2026-08-05T00:00:00Z', subject = component:store;
UPDATE memory:mine_rule SET uses += 100;
CREATE event:bad SET kind = 12345;
COMMIT;
"#;

/// A committed write used to bound the live-query assertion: once this arrives
/// on the feed, everything the earlier turns were going to emit already has.
pub const SENTINEL_WRITE: &str = r#"
CREATE memory:sentinel SET category = 'context', scope = 'project:srswart/totem',
    body = 'sentinel', embedding = [0.0, 1.0, 0.0, 0.0],
    created_at = d'2026-08-05T00:00:00Z', subject = component:store;
"#;

/// Generous on purpose: this bounds a hang, it does not define the assertion.
const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Probe vector pointing along the readable cluster's axis.
fn probe() -> Vec<f32> {
    vec![0.0, 1.0, 0.0, 0.0]
}

fn readable_scopes() -> Vec<String> {
    vec!["project:srswart/totem".to_string()]
}

/// The temporal cutoff, as a real `datetime`. Binding this as a string instead
/// is the trap [`recall_bodies_with_string_cutoff`] documents.
fn since() -> Datetime {
    "2026-06-01T00:00:00Z"
        .parse()
        .expect("cutoff is a valid RFC 3339 timestamp")
}

/// Install the toy schema and seed it. Statement-level errors are surfaced
/// rather than swallowed — a silently failing `DEFINE` would make every later
/// assertion meaningless.
pub async fn install_toy_dataset<C: Connection>(db: &Surreal<C>) -> surrealdb::Result<()> {
    db.query(TOY_SCHEMA).await?.check()?;
    db.query(TOY_SEED).await?.check()?;
    Ok(())
}

/// Drop everything this spike writes, so a parity run can be pointed at a
/// scratch namespace on a real server repeatedly.
pub async fn reset<C: Connection>(db: &Surreal<C>) -> surrealdb::Result<()> {
    db.query(
        "REMOVE TABLE IF EXISTS memory;
         REMOVE TABLE IF EXISTS component;
         REMOVE TABLE IF EXISTS event;
         REMOVE TABLE IF EXISTS derived_from",
    )
    .await?
    .check()?;
    Ok(())
}

/// Experiment 1 — one round trip assembles document + vector + graph + temporal
/// context, and resolves the scope filter itself.
///
/// Returns the bodies that came back, in rank order.
pub async fn verify_one_round_trip<C: Connection>(
    db: &Surreal<C>,
) -> surrealdb::Result<Vec<String>> {
    let mut response = db
        .query(RECALL_QUERY)
        .bind(("probe", probe()))
        .bind(("scopes", readable_scopes()))
        .bind(("since", since()))
        .await?
        .check()?;
    let rows: Value = response.take(0)?;
    let rows = rows.into_array().expect("recall returns an array");

    let mut bodies = Vec::new();
    for row in rows.iter() {
        let row = row.clone().into_object().expect("recall rows are objects");
        let body = field_string(&row, "body");

        // Every projected model must be populated, not merely present: the
        // record link resolved to the subject entity's name, the vector
        // operator handed back its distance, and the top hit carried the
        // episode it was derived from.
        assert_eq!(
            field_string(&row, "subject_name"),
            "Totem Store",
            "record link did not resolve inside the recall query"
        );
        assert!(
            row.get("distance").is_some(),
            "vector distance missing from the recall projection"
        );
        if body == "scope isolation is enforced in the store" {
            let derived = row
                .get("derived_from_bodies")
                .and_then(|v| v.clone().into_array().ok())
                .expect("derived_from_bodies is an array");
            assert_eq!(
                derived.len(),
                1,
                "graph traversal did not resolve inside the recall query"
            );
        }

        bodies.push(body);
    }
    Ok(bodies)
}

/// Experiment 2 — the temporal cutoff only filters when it is bound as a
/// `datetime`.
///
/// Binding the same instant as a string produces no parse error, no type error,
/// and no filtering: SurrealQL compares values of different types by type rank,
/// so `created_at > $since` is constant-true against a string. Returns the
/// bodies recalled with the string binding, which is a superset of the correct
/// answer.
pub async fn recall_bodies_with_string_cutoff<C: Connection>(
    db: &Surreal<C>,
) -> surrealdb::Result<Vec<String>> {
    let mut response = db
        .query(RECALL_QUERY)
        .bind(("probe", probe()))
        .bind(("scopes", readable_scopes()))
        .bind(("since", "2026-06-01T00:00:00Z".to_string()))
        .await?
        .check()?;
    let rows: Value = response.take(0)?;
    let rows = rows.into_array().expect("recall returns an array");
    Ok(rows
        .iter()
        .map(|row| {
            let row = row.clone().into_object().expect("recall rows are objects");
            field_string(&row, "body")
        })
        .collect())
}

/// Experiment 3 — where the scope predicate actually runs.
///
/// The foreign records are nearer the probe than everything readable, so if the
/// engine truncated to K by distance and filtered afterwards, the readable rows
/// would disappear. Returns the query plan so the caller can assert that the
/// predicate reached the index scan rather than being applied by the caller.
pub async fn explain_scoped_knn<C: Connection>(db: &Surreal<C>) -> surrealdb::Result<String> {
    let mut response = db
        .query(format!("{RECALL_QUERY} EXPLAIN FULL"))
        .bind(("probe", probe()))
        .bind(("scopes", readable_scopes()))
        .bind(("since", since()))
        .await?
        .check()?;
    let plan: Value = response.take(0)?;
    Ok(format!("{plan:?}"))
}

/// Experiment 4 — a turn's three writes commit together, and a turn whose last
/// write fails leaves nothing behind.
pub async fn verify_transaction_atomicity<C: Connection>(db: &Surreal<C>) -> surrealdb::Result<()> {
    db.query(TURN_WRITE).await?.check()?;
    assert_eq!(
        count(db, "memory:decision").await?,
        1,
        "decision not committed"
    );
    assert_eq!(
        count(db, "event:promotion").await?,
        1,
        "triggered event not committed"
    );
    assert_eq!(uses(db).await?, 1, "entity update not committed");

    // The failing turn must abort as a unit.
    let failed = db.query(TURN_WRITE_FAILING).await?.check();
    assert!(failed.is_err(), "schema-violating turn was accepted");
    assert_eq!(
        count(db, "memory:orphan").await?,
        0,
        "rolled-back CREATE survived"
    );
    assert_eq!(uses(db).await?, 1, "rolled-back UPDATE survived");
    Ok(())
}

/// Experiment 5 — live queries fire on the engine under test, and only for
/// committed writes.
///
/// The subscription is opened before the writes, so the caller must not have
/// run [`verify_transaction_atomicity`] on this database already.
///
/// Proving the *absence* of a notification for the rolled-back turn without
/// depending on a quiet period: a sentinel record is committed afterwards, and
/// the feed is read until the sentinel arrives. Anything the aborted turn
/// emitted would have to appear before it. That keeps the assertion
/// deterministic under load rather than tuned to a drain timeout.
pub async fn verify_live_query<C: Connection>(
    db: &Surreal<C>,
) -> surrealdb::Result<Vec<(Action, String)>> {
    let mut stream = db
        .query("LIVE SELECT * FROM memory WHERE scope = 'project:srswart/totem'")
        .await?
        .check()?
        .stream::<Value>(0)?;

    db.query(TURN_WRITE).await?.check()?;
    let aborted = db.query(TURN_WRITE_FAILING).await?.check();
    assert!(aborted.is_err(), "schema-violating turn was accepted");
    db.query(SENTINEL_WRITE).await?.check()?;

    let mut seen = Vec::new();
    loop {
        let notification = tokio::time::timeout(NOTIFICATION_TIMEOUT, stream.next())
            .await
            .expect("live feed went quiet before the sentinel arrived")
            .expect("live feed closed before the sentinel arrived")?;
        // Panic rather than default to an empty id: the caller uses these ids to
        // assert that a rolled-back record *never* appeared, and `"".contains(..)`
        // would make that assertion pass vacuously.
        let id = notification
            .data
            .clone()
            .into_object()
            .ok()
            .and_then(|row| row.get("id").map(|id| format!("{id:?}")))
            .unwrap_or_else(|| {
                panic!(
                    "live notification carried no `id`; cannot tell which record it was: {:?}",
                    notification.data
                )
            });
        let is_sentinel = id.contains("sentinel");
        seen.push((notification.action, id));
        if is_sentinel {
            return Ok(seen);
        }
    }
}

fn field_string(row: &Object, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.clone().into_string().ok())
        .unwrap_or_else(|| panic!("`{key}` missing or not a string in recall row"))
}

async fn count<C: Connection>(db: &Surreal<C>, record: &str) -> surrealdb::Result<usize> {
    let mut response = db
        .query(format!("SELECT id FROM {record}"))
        .await?
        .check()?;
    let rows: Value = response.take(0)?;
    Ok(rows.into_array().expect("select returns an array").len())
}

async fn uses<C: Connection>(db: &Surreal<C>) -> surrealdb::Result<i64> {
    let mut response = db
        .query("SELECT VALUE uses FROM memory:mine_rule")
        .await?
        .check()?;
    let rows: Value = response.take(0)?;
    let rows = rows.into_array().expect("select returns an array");
    let first = rows.iter().next().expect("mine_rule exists").clone();
    Ok(first.into_int().expect("uses is an int"))
}
