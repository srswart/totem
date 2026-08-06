//! The gateway binary: the one long-running Totem process (DEP-001).
//!
//! With `TOTEM_DATA_DIR` set (and the `rocksdb` feature compiled in), the
//! gateway owns an embedded on-disk store at that directory — the durable
//! shared instance every other surface talks to. The engine's lock makes the
//! gateway the store's sole owner physically: a second process opening the
//! same directory fails to start. Without `TOTEM_DATA_DIR` the gateway runs
//! the embedded in-memory engine — explicitly labelled EPHEMERAL — which is
//! the demo/test mode, not a deployment.

use std::sync::Arc;

use surrealdb::engine::local::Db;
use totem_gateway::AppState;
use totem_store::{DeterministicEmbedder, Store};

/// Connect the store per DEP-001: durable when configured, loudly ephemeral
/// otherwise — and a hard refusal when configured for durability the binary
/// cannot deliver.
async fn connect_store() -> Store<Db> {
    match std::env::var("TOTEM_DATA_DIR") {
        Ok(dir) => {
            #[cfg(feature = "rocksdb")]
            {
                let store = Store::on_disk(std::path::Path::new(&dir))
                    .await
                    .unwrap_or_else(|err| {
                        eprintln!(
                            "totem-gateway: cannot open the data directory at {dir}: {err}\n\
                             If another gateway is running against it, that lock is DEP-001's \
                             single-owner rule doing its job."
                        );
                        std::process::exit(1);
                    });
                println!("totem-gateway store: durable (RocksDB at {dir})");
                store
            }
            #[cfg(not(feature = "rocksdb"))]
            {
                eprintln!(
                    "totem-gateway: TOTEM_DATA_DIR is set ({dir}) but this binary was built \
                     without the `rocksdb` feature. Refusing to start: an in-memory gateway \
                     that looks configured for durability would lose the team's memory on \
                     exit. Rebuild with `--features rocksdb`, or unset TOTEM_DATA_DIR to run \
                     explicitly ephemeral."
                );
                std::process::exit(1);
            }
        }
        Err(_) => {
            let store = Store::in_memory()
                .await
                .expect("the embedded engine connects");
            println!(
                "totem-gateway store: EPHEMERAL in-memory — memories are lost on exit \
                 (set TOTEM_DATA_DIR with a rocksdb-featured build for durability)"
            );
            store
        }
    }
}

#[tokio::main]
async fn main() {
    let store = connect_store().await;
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
    println!(
        "totem-gateway listening on {}",
        listener.local_addr().expect("listener has a local address")
    );
    axum::serve(listener, app)
        .await
        .expect("the server runs until shut down");
}
