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
use totem_store::{Embedder, Store};

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

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState")
            .field("store", &self.store)
            .field("embedder_model", &self.embedder.model_name())
            .finish()
    }
}
