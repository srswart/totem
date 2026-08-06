//! The gateway binary: an embedded, in-memory store behind the REST router.
//!
//! In-memory and non-persistent on purpose, for now: production deployment
//! topology (embedded vs. server SurrealDB, where state survives a restart)
//! is an open question (docs/solution-intent.md §9), and this advance's scope
//! is the API surface, not that decision. Whoever resolves the topology
//! question wires `Store::from_connection` onto a durable engine here instead.

use std::sync::Arc;

use totem_gateway::AppState;
use totem_store::{DeterministicEmbedder, Store};

#[tokio::main]
async fn main() {
    let store = Store::in_memory()
        .await
        .expect("the embedded engine connects");
    store.migrate().await.expect("migrations apply");

    // The deterministic, non-semantic embedder: real quality
    // (BGE-small-en-v1.5 via `fastembed`, EMB-004) needs a model download this
    // sandbox's egress policy blocks, so it stays behind the store's
    // off-by-default `fastembed` feature until a workstation or CI runner
    // with hub access builds this binary with it enabled.
    let embedder = Arc::new(DeterministicEmbedder::new());

    let state = AppState { store, embedder };
    let app = totem_gateway::router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8787")
        .await
        .expect("binds the gateway's listening port");
    println!("totem-gateway listening on {}", listener.local_addr().expect("listener has a local address"));
    axum::serve(listener, app)
        .await
        .expect("the server runs until shut down");
}
