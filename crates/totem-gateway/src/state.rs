//! Shared state every handler reads: the store connection and the embedder
//! that turns write bodies and recall queries into vectors.
//!
//! Embedding placement is gateway-on-write per
//! [docs/tech-direction/embeddings.md](../../../docs/tech-direction/embeddings.md)
//! §4 — not deferred to a curator batch job, so a freshly-saved memory is
//! recallable by vector search immediately.

use std::fmt;
use std::sync::Arc;

use surrealdb::engine::local::Db;
use totem_store::{DeterministicEmbedder, Embedder, Store, StoreResult};

/// State cloned into every request handler.
///
/// [`Store`] is cheap to clone (it wraps a `Surreal<C>` connection handle),
/// and the embedder is behind an `Arc` since trait objects are not `Clone`.
#[derive(Clone)]
pub struct AppState {
    /// The store connection. `Db` (the embedded engine) for now: production
    /// deployment topology is an open question
    /// (docs/solution-intent.md §9), so this binary does not attempt to
    /// choose a server engine ahead of that decision.
    pub store: Store<Db>,
    /// The embedder every `/save` and `/recall` call uses.
    pub embedder: Arc<dyn Embedder>,
}

impl AppState {
    /// State over a fresh, migrated, embedded in-memory store, with the
    /// deterministic embedder.
    ///
    /// Both binaries (`totem-gateway`, `mcp_stdio`) and the integration tests
    /// need exactly this, and had each spelled it out: three places to keep in
    /// step every time the state gains a field.
    ///
    /// In-memory and non-persistent on purpose, for now: production deployment
    /// topology (embedded vs. server SurrealDB, where state survives a restart)
    /// is an open question (docs/solution-intent.md §9). Whoever resolves it
    /// adds the durable constructor beside this one rather than changing what
    /// this one means.
    pub async fn in_memory() -> StoreResult<Self> {
        let store = Store::in_memory().await?;
        store.migrate().await?;
        Ok(Self {
            store,
            // The deterministic, non-semantic embedder: real quality
            // (BGE-small-en-v1.5 via `fastembed`, EMB-004) needs a model
            // download this sandbox's egress policy blocks, so it stays behind
            // the store's off-by-default `fastembed` feature until a
            // workstation or CI runner with hub access builds with it enabled.
            embedder: Arc::new(DeterministicEmbedder::new()),
        })
    }
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState")
            .field("store", &self.store)
            .field("embedder_model", &self.embedder.model_name())
            .finish()
    }
}
