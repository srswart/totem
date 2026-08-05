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
//! With the feature off this file compiles to nothing; with the feature on but
//! no URL set, the test reports the skip rather than passing silently.
#![cfg(feature = "server-parity")]

use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Ws;
use surrealdb::opt::auth::Root;
use surrealdb::types::Action;
use totem_store_spike::{
    explain_scoped_knn, install_toy_dataset, reset, verify_live_query, verify_one_round_trip,
    verify_transaction_atomicity,
};

/// Every capability the embedded engine was shown to have, asserted again over
/// the WebSocket protocol. Divergence here is the finding, not a flake.
#[tokio::test(flavor = "multi_thread")]
async fn server_mode_matches_the_embedded_engine() -> surrealdb::Result<()> {
    let Ok(url) = std::env::var("TOTEM_SPIKE_SURREAL_URL") else {
        eprintln!("SKIPPED: set TOTEM_SPIKE_SURREAL_URL to a ws:// endpoint to run parity checks");
        return Ok(());
    };
    let url = url.trim_start_matches("ws://").to_string();

    let db = Surreal::new::<Ws>(url).await?;
    let user = std::env::var("TOTEM_SPIKE_SURREAL_USER").unwrap_or_else(|_| "root".to_string());
    let pass = std::env::var("TOTEM_SPIKE_SURREAL_PASS").unwrap_or_else(|_| "root".to_string());
    db.signin(Root {
        username: user,
        password: pass,
    })
    .await?;
    db.use_ns("totem").use_db("spike_parity").await?;

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
