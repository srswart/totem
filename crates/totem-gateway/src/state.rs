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

use crate::auth::TokenRegistry;

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
    /// Where the console bundle lives, when this deployment serves one
    /// (ADV-GATEWAY-010). `None` is an API-only gateway — a legitimate
    /// configuration, not a broken one. Held in state rather than read from
    /// the environment inside the router, so tests set it per-instance
    /// instead of racing on a process-global.
    pub console_dir: Option<std::path::PathBuf>,
    /// The console's public OAuth client id, when it signs humans in
    /// (ADV-GATEWAY-010). Public by construction — a PKCE client has no
    /// secret, which is why this may be served to a browser.
    pub console_client_id: Option<String>,
    /// Where the authorization server returns the browser after sign-in.
    pub console_redirect_uri: Option<String>,
    /// The OAuth resource-server verifier, when the deployment configures one
    /// (ADV-GATEWAY-013). `None` on a workstation with no authorization
    /// server, where static bearer credentials are the only path.
    pub oauth: Option<std::sync::Arc<crate::OAuthVerifier>>,
    /// The credentials this gateway accepts from remote callers
    /// (ADV-GATEWAY-003). Empty by default, so a gateway that has been given
    /// no credentials refuses every remote request rather than serving them
    /// unauthenticated.
    pub tokens: TokenRegistry,
}

impl AppState {
    /// State over an already-connected, already-migrated store.
    ///
    /// The engine is the caller's choice — DEP-001 leaves the gateway binary
    /// to pick durable (RocksDB at `TOTEM_DATA_DIR`) or ephemeral
    /// (ADV-INFRA-001) — but everything *else* the state holds is decided
    /// here, in one place, so a new field does not have to be threaded through
    /// every construction site by hand.
    pub fn over(store: Store<Db>) -> Self {
        Self {
            store,
            console_dir: None,
            console_client_id: None,
            console_redirect_uri: None,
            // Configured by the binary from the environment when a deployment
            // has an authorization server; absent on a workstation.
            oauth: None,
            // The deterministic, non-semantic embedder: real quality
            // (BGE-small-en-v1.5 via `fastembed`, EMB-004) needs a model
            // download this sandbox's egress policy blocks, so it stays behind
            // the store's off-by-default `fastembed` feature until a
            // workstation or CI runner with hub access builds with it enabled.
            embedder: Arc::new(DeterministicEmbedder::new()),
            // Empty: a gateway that has been given no credentials refuses
            // every remote request rather than serving them unauthenticated.
            tokens: TokenRegistry::new(),
        }
    }

    /// State over a fresh, migrated, embedded in-memory store.
    ///
    /// The convenience the `mcp_stdio` binary and the integration tests want,
    /// each of which had spelled the sequence out for itself. The gateway
    /// binary does not use it: DEP-001 gives that process a store choice to
    /// make, so it connects its own and hands it to [`AppState::over`].
    pub async fn in_memory() -> StoreResult<Self> {
        let store = Store::in_memory().await?;
        store.migrate().await?;
        Ok(Self::over(store))
    }
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState")
            .field("store", &self.store)
            .field("embedder_model", &self.embedder.model_name())
            .field("tokens", &self.tokens)
            .finish()
    }
}
