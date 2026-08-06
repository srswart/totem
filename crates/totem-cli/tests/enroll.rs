//! `totem enroll`, end-to-end against a real (bound-socket, not oneshot)
//! gateway: `totem-cli` is a separate process from `totem-gateway` in
//! practice, so this drives the actual HTTP client path
//! (docs/solution-intent.md §3.3; ADV-CLI-001), the way `totem-mcp-spike`'s
//! streamable-HTTP round trip does for MCP.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::net::TcpListener;
use totem_cli::enroll::{self, EnrollError};
use totem_gateway::AppState;
use totem_store::{DeterministicEmbedder, Store};

fn arrive_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../arrive")
}

async fn spawn_gateway() -> (String, Store<surrealdb::engine::local::Db>) {
    let store = Store::in_memory().await.expect("embedded engine connects");
    store.migrate().await.expect("migrations apply");
    let state = AppState {
        store: store.clone(),
        embedder: Arc::new(DeterministicEmbedder::new()),
    };
    let app = totem_gateway::router(state);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds a loopback port");
    let addr: SocketAddr = listener.local_addr().expect("listener has a local address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("http://{addr}"), store)
}

#[tokio::test]
async fn enrolling_this_repo_against_a_real_gateway_populates_its_landscape() {
    let (gateway_url, store) = spawn_gateway().await;
    let client = reqwest::Client::new();

    let summary = enroll::enroll(&client, &gateway_url, &arrive_root(), "test:enroll")
        .await
        .expect("enroll succeeds");
    assert_eq!(summary.systems, 1);
    assert!(summary.advances >= 23);

    let view = store
        .landscape()
        .view("058-totem")
        .await
        .expect("landscape view succeeds");
    assert_eq!(
        view.repo.map(|repo| repo.id),
        Some("058-totem".to_string())
    );
}

#[tokio::test]
async fn enrolling_against_an_unreachable_gateway_reports_plainly() {
    // Bind, read the address, then drop the listener: nothing is listening,
    // so the connection is refused immediately rather than hanging.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds a loopback port");
    let addr = listener.local_addr().expect("listener has a local address");
    drop(listener);

    let client = reqwest::Client::new();
    let error = enroll::enroll(
        &client,
        &format!("http://{addr}"),
        &arrive_root(),
        "test:enroll",
    )
    .await
    .expect_err("nothing is listening");
    assert!(matches!(error, EnrollError::Request { .. }), "{error:?}");
}

#[tokio::test]
async fn enrolling_a_missing_arrive_directory_is_reported_plainly() {
    let (gateway_url, _store) = spawn_gateway().await;
    let client = reqwest::Client::new();

    let error = enroll::enroll(
        &client,
        &gateway_url,
        Path::new("/nonexistent/totem-cli-fixture"),
        "test:enroll",
    )
    .await
    .expect_err("a missing /arrive/ directory cannot be ingested");
    assert!(matches!(error, EnrollError::Ingest(_)), "{error:?}");
}
