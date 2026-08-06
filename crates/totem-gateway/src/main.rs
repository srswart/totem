//! The gateway binary: an embedded, in-memory store behind the REST router.
//!
//! In-memory and non-persistent on purpose, for now: production deployment
//! topology (embedded vs. server SurrealDB, where state survives a restart)
//! is an open question (docs/solution-intent.md §9), and this advance's scope
//! is the API surface, not that decision. Whoever resolves the topology
//! question wires `Store::from_connection` onto a durable engine in
//! [`AppState::in_memory`]'s place.

use totem_gateway::AppState;

#[tokio::main]
async fn main() {
    let state = AppState::in_memory()
        .await
        .expect("the embedded engine connects and migrations apply");
    let app = totem_gateway::router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8787")
        .await
        .expect("binds the gateway's listening port");
    println!(
        "totem-gateway listening on {}",
        listener.local_addr().expect("listener has a local address")
    );
    axum::serve(listener, app)
        .await
        .expect("the server runs until shut down");
}
