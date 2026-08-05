//! ADV-STORE-004 engine parity — the same experiments, run against a real
//! `surreal start` server instead of the embedded `kv-mem` engine.
//!
//! Opt-in and off by default, because no `surreal` server exists in the cloud
//! sandbox this project's agents run in, and a test that assumes one would hang
//! there. To run it:
//!
//! ```text
//! surreal start --user root --pass root memory &
//! TOTEM_SPIKE_SURREAL_URL=ws://127.0.0.1:8000 \
//!   cargo test -p totem-store-spike --features server-parity --test server_parity
//! ```
//!
//! With the feature off this file compiles to nothing, so a default
//! `cargo test --workspace` can never wait on a server. With the feature on,
//! the run has explicitly asked for parity, so a missing URL **fails** — a green
//! parity test that checked nothing would be worse than no parity test.
#![cfg(feature = "server-parity")]

use surrealdb::Surreal;
use surrealdb::engine::remote::http::Http;
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::{Database, Root};
use surrealdb::types::Action;
use totem_store_spike::{
    PROBE_BASELINE, PROBE_NETWORK, PROBE_SCRIPTING, SENTINEL_WRITE, explain_scoped_knn,
    install_toy_dataset, probe_expression, reset, verify_live_query, verify_one_round_trip,
    verify_transaction_atomicity,
};

/// The server host, without a scheme. Failing loudly on an unset URL is the
/// property ADV-STORE-004 established and ADV-STORE-006 must not lose: a green
/// parity run that checked nothing is worse than no parity run.
fn server_host() -> String {
    let url = std::env::var("TOTEM_SPIKE_SURREAL_URL").expect(
        "the `server-parity` feature is enabled but TOTEM_SPIKE_SURREAL_URL is unset; \
         start a server (`surreal start --user root --pass root memory`) and point this \
         at it (e.g. ws://127.0.0.1:8000), or run without --features server-parity",
    );
    url.trim_start_matches("ws://").to_string()
}

fn root_credentials() -> Root {
    Root {
        username: std::env::var("TOTEM_SPIKE_SURREAL_USER").unwrap_or_else(|_| "root".to_string()),
        password: std::env::var("TOTEM_SPIKE_SURREAL_PASS").unwrap_or_else(|_| "root".to_string()),
    }
}

/// Root-authenticated WebSocket connection on its own database, so the tests
/// in this file can run concurrently without seeing each other's writes.
async fn root_connection(database: &str) -> surrealdb::Result<Surreal<Client>> {
    let db = Surreal::new::<Ws>(server_host()).await?;
    db.signin(root_credentials()).await?;
    db.use_ns("totem").use_db(database).await?;
    Ok(db)
}

/// Every capability the embedded engine was shown to have, asserted again over
/// the WebSocket protocol. Divergence here is the finding, not a flake.
#[tokio::test(flavor = "multi_thread")]
async fn server_mode_matches_the_embedded_engine() -> surrealdb::Result<()> {
    let db = root_connection("spike_parity").await?;

    // The findings are version-specific (query-plan strings, `<|K,EF|>`
    // behaviour), so a version-mismatched server answers a different question
    // than ADV-STORE-004 asked. Enforce the pin rather than documenting it.
    let version = db.version().await?;
    assert_eq!(
        (version.major, version.minor, version.patch),
        (3, 2, 4),
        "server version {version:?} does not match the client pin =3.2.4; \
         this run would not be comparable to the ADV-STORE-004 findings"
    );
    println!("server version: {version:?}");

    reset(&db).await?;
    install_toy_dataset(&db).await?;

    let bodies = verify_one_round_trip(&db).await?;
    assert_eq!(
        bodies,
        vec![
            "scope isolation is enforced in the store".to_string(),
            "the turn that produced the rule".to_string(),
        ],
        "recall ranking differs from the embedded engine"
    );

    let plan = explain_scoped_knn(&db).await?;
    assert!(
        plan.contains("KnnScan") && plan.contains("scope INSIDE"),
        "server did not push the scope predicate into the index scan: {plan}"
    );

    // Same shape as the embedded assertion, and order-insensitive within the
    // transaction for the same reason (TD-008).
    let seen = verify_live_query(&db).await?;
    assert_eq!(
        seen.len(),
        3,
        "live feed over WebSocket differs from the embedded engine: {seen:?}"
    );
    assert!(
        !seen.iter().any(|(_, id)| id.contains("orphan")),
        "server published a rolled-back write to the live feed: {seen:?}"
    );
    let counts = |action: Action| seen.iter().filter(|(a, _)| *a == action).count();
    assert_eq!(
        (counts(Action::Create), counts(Action::Update)),
        (2, 1),
        "live feed over WebSocket differs from the embedded engine: {seen:?}"
    );

    reset(&db).await?;
    install_toy_dataset(&db).await?;
    verify_transaction_atomicity(&db).await?;

    reset(&db).await?;
    Ok(())
}

/// The surface an embedded instance does not have at all: authentication.
/// Wrong credentials must be refused at signin, not at first query.
#[tokio::test(flavor = "multi_thread")]
async fn signin_refuses_bad_credentials() -> surrealdb::Result<()> {
    let db = Surreal::new::<Ws>(server_host()).await?;
    let refused = db
        .signin(Root {
            username: "root".to_string(),
            password: "definitely-not-the-password".to_string(),
        })
        .await;
    let refusal = refused.expect_err("server accepted wrong root credentials");
    println!("bad-credential refusal: {refusal}");

    // Positive control on the same connection object: correct credentials
    // succeed, so the refusal above came from the credentials, not the setup.
    db.signin(root_credentials()).await?;
    Ok(())
}

/// ADV-GATEWAY-003's least-privilege model needs to know what a restricted
/// server user can and cannot do. Executed answer (TD-011): a VIEWER-role
/// database user runs the full recall surface identically to root, gets a hard
/// IAM error for config/DDL actions — but its *data writes are silently
/// discarded*: `CREATE` returns OK with an empty result and persists nothing.
/// Error-checking alone therefore cannot detect a write dropped by
/// authorization, and DB roles do nothing for Totem's scope isolation (the
/// viewer reads every scope): both stay the store layer's job.
#[tokio::test(flavor = "multi_thread")]
async fn viewer_role_reads_fully_ddl_errors_and_writes_vanish_silently() -> surrealdb::Result<()> {
    let root = root_connection("spike_parity_auth").await?;
    reset(&root).await?;
    install_toy_dataset(&root).await?;
    root.query(
        "DEFINE USER OVERWRITE spike_viewer ON DATABASE \
         PASSWORD 'spike-viewer-throwaway' ROLES VIEWER",
    )
    .await?
    .check()?;

    let viewer = Surreal::new::<Ws>(server_host()).await?;
    viewer
        .signin(Database {
            namespace: "totem".to_string(),
            database: "spike_parity_auth".to_string(),
            username: "spike_viewer".to_string(),
            password: "spike-viewer-throwaway".to_string(),
        })
        .await?;
    viewer.use_ns("totem").use_db("spike_parity_auth").await?;

    // Read parity under least privilege: the same one-round-trip recall,
    // including the vector index, graph traversal, and record link.
    let bodies = verify_one_round_trip(&viewer).await?;
    assert_eq!(
        bodies,
        vec![
            "scope isolation is enforced in the store".to_string(),
            "the turn that produced the rule".to_string(),
        ],
        "recall under a VIEWER-role user differs from root"
    );

    // The trap this test exists to pin: the viewer's CREATE is *accepted* —
    // no error anywhere — and silently persists nothing.
    assert_eq!(
        probe_expression(&viewer, SENTINEL_WRITE).await,
        Ok(()),
        "3.2.4 silently discarded a viewer write; an error here means the \
         semantics changed and TD-011 must be revisited"
    );
    assert_eq!(
        count_as(&root, "memory:sentinel").await?,
        0,
        "a VIEWER-role user's CREATE was persisted"
    );

    // Config/DDL actions, by contrast, fail loudly with an IAM error.
    let ddl_refusal = probe_expression(&viewer, "DEFINE TABLE viewer_ddl_probe")
        .await
        .expect_err("a VIEWER-role user was allowed to DEFINE TABLE");
    println!("viewer ddl refusal: {ddl_refusal}");
    assert!(
        ddl_refusal.contains("Not enough permissions"),
        "unexpected DDL refusal text: {ddl_refusal}"
    );

    // Negative control: the identical CREATE persists as root, so the silent
    // discard above is the role's doing, not a broken statement.
    root.query(SENTINEL_WRITE).await?.check()?;
    assert_eq!(
        count_as(&root, "memory:sentinel").await?,
        1,
        "control CREATE as root did not persist"
    );

    reset(&root).await?;
    root.query("REMOVE USER IF EXISTS spike_viewer ON DATABASE")
        .await?
        .check()?;
    Ok(())
}

/// Root-side existence check for the silent-discard assertion.
async fn count_as(db: &Surreal<Client>, record: &str) -> surrealdb::Result<usize> {
    let mut response = db
        .query(format!("SELECT id FROM {record}"))
        .await?
        .check()?;
    let rows: surrealdb::types::Value = response.take(0)?;
    Ok(rows.into_array().expect("select returns an array").len())
}

/// The server half of the capability comparison (embedded half:
/// `tests/embedded.rs`). A default `surreal start` must refuse scripting and
/// outbound network calls even for root — those are capability flags, not
/// roles — while the capability-free baseline proves the probes can pass.
#[tokio::test(flavor = "multi_thread")]
async fn capability_defaults_refuse_scripting_and_network_even_for_root() -> surrealdb::Result<()> {
    let db = root_connection("spike_parity_caps").await?;

    assert_eq!(
        probe_expression(&db, PROBE_BASELINE).await,
        Ok(()),
        "capability-free baseline failed; probe harness is broken"
    );
    let scripting = probe_expression(&db, PROBE_SCRIPTING)
        .await
        .expect_err("a default `surreal start` executed an embedded script");
    let network = probe_expression(&db, PROBE_NETWORK)
        .await
        .expect_err("a default `surreal start` made an outbound network call");
    println!("server scripting refusal: {scripting}");
    println!("server network refusal: {network}");
    Ok(())
}

/// ADV-STORE-004 established from SDK source that live queries are unsupported
/// over the HTTP remote protocol; this executes it. The gateway/console must
/// hold a WebSocket (or embedded) connection for live feeds — an HTTP
/// connection refuses them.
#[tokio::test(flavor = "multi_thread")]
async fn live_queries_are_refused_over_the_http_protocol() -> surrealdb::Result<()> {
    // The WebSocket control below needs the table to exist — `LIVE SELECT`
    // over WS validates the table, while the HTTP transport refuses before
    // even looking (the refusal fires identically on this empty database).
    let ws = root_connection("spike_parity_http").await?;
    ws.query("DEFINE TABLE OVERWRITE memory SCHEMALESS")
        .await?
        .check()?;

    let http = Surreal::new::<Http>(server_host()).await?;
    http.signin(root_credentials()).await?;
    http.use_ns("totem").use_db("spike_parity_http").await?;

    let refusal = probe_expression(&http, "LIVE SELECT * FROM memory")
        .await
        .expect_err("the HTTP protocol accepted a live query");
    println!("http live-query refusal: {refusal}");
    // The server refuses the raw statement as "Unable to perform the realtime
    // query"; the SDK's own `.live()` path refuses client-side with "does not
    // support live queries". Either way the refusal names the realtime nature.
    let lower = refusal.to_lowercase();
    assert!(
        lower.contains("realtime") || lower.contains("live"),
        "refusal does not name live/realtime queries: {refusal}"
    );

    // Negative control: the identical statement on the identical database is
    // accepted over WebSocket, so the refusal is the transport's, not the
    // statement's.
    ws.query("LIVE SELECT * FROM memory").await?.check()?;

    ws.query("REMOVE TABLE IF EXISTS memory").await?.check()?;
    Ok(())
}
