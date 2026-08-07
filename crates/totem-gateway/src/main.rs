//! The gateway binary: the one long-running Totem process (DEP-001), serving
//! the authenticated application — REST plus MCP over streamable HTTP, every
//! route behind a bearer credential (ADV-GATEWAY-003).
//!
//! With `TOTEM_DATA_DIR` set (and the `rocksdb` feature compiled in), the
//! gateway owns an embedded on-disk store at that directory — the durable
//! shared instance every other surface talks to. The engine's lock makes the
//! gateway the store's sole owner physically: a second process opening the
//! same directory fails to start. Without `TOTEM_DATA_DIR` the gateway runs
//! the embedded in-memory engine — explicitly labelled EPHEMERAL — which is
//! the demo/test mode, not a deployment.
//!
//! # The bootstrap credential
//!
//! The registry starts empty, and an empty registry refuses every request.
//! That is deliberate: a gateway that served unauthenticated callers whenever
//! it had no credentials configured would fail open exactly when it is least
//! configured. One credential can be seeded from the environment to get an
//! operator in; issuing the rest is the console's and CLI's job
//! (ADV-CONSOLE-002, ADV-CLI-001).
//!
//! Credentials do **not** yet get DEP-001's durability: the registry is
//! process-local, so a restart of this durable gateway still forgets every
//! credential and needs its bootstrap credential re-seeded. Giving the
//! registry the same on-disk home as the store is follow-up work, called out
//! in ADV-GATEWAY-003's residual risks.

use std::env;

use surrealdb::engine::local::Db;
use totem_gateway::{AppState, AuthError, TokenGrant, TokenRegistry};
use totem_store::Store;

const TOKEN_VAR: &str = "TOTEM_BOOTSTRAP_TOKEN";
const REPO_VAR: &str = "TOTEM_BOOTSTRAP_REPO";
const SCOPE_VAR: &str = "TOTEM_BOOTSTRAP_SCOPE";
const ACTOR_VAR: &str = "TOTEM_BOOTSTRAP_ACTOR";

/// Connect the store per DEP-001: durable when configured, loudly ephemeral
/// otherwise — and a hard refusal when configured for durability the binary
/// cannot deliver.
async fn connect_store() -> Store<Db> {
    match env::var("TOTEM_DATA_DIR") {
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

/// Register the credential named by the environment, if one is named.
///
/// Takes the token text rather than issuing one so an operator can hand the
/// gateway a credential `totem credential issue` already produced
/// (ADV-CLI-001) — the two describe the same grant.
///
/// Returns the grant that was registered, or `None` when no bootstrap
/// credential was configured. A *partially* configured one is an error, not a
/// silent skip: half-set variables mean someone intended to authenticate.
fn bootstrap(tokens: &TokenRegistry) -> Result<Option<(TokenGrant, String)>, AuthError> {
    let configured: Vec<Option<String>> = [TOKEN_VAR, REPO_VAR, SCOPE_VAR, ACTOR_VAR]
        .iter()
        .map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
        .collect();

    if configured.iter().all(Option::is_none) {
        return Ok(None);
    }
    if configured.iter().any(Option::is_none) {
        return Err(AuthError::InvalidBinding(format!(
            "a bootstrap credential needs all of {TOKEN_VAR}, {REPO_VAR}, {SCOPE_VAR}, {ACTOR_VAR}"
        )));
    }

    let [token, repo, scope, actor] = <[Option<String>; 4]>::try_from(configured)
        .expect("four variables were collected")
        .map(|value| value.expect("every variable was checked as present"));

    // Issue-then-replace rather than constructing the grant directly: `issue`
    // is what refuses an over-scoped binding (a scope naming another repo or
    // another actor), and a bootstrap credential must not be the one that
    // skips that check.
    let issued = tokens.issue(&repo, &scope, &actor, None)?;
    let grant = tokens
        .verify(&issued, chrono::Utc::now())
        .expect("a credential this call just issued verifies");
    tokens.revoke(&issued);
    tokens.register(&token, grant.clone());

    Ok(Some((grant, TokenRegistry::fingerprint_of(&token))))
}

#[tokio::main]
async fn main() {
    let store = connect_store().await;
    store.migrate().await.expect("migrations apply");
    let mut state = AppState::over(store);

    // OAuth resource-server mode (ADV-GATEWAY-013), when the deployment
    // configures an authorization server. Absent it, static bearer
    // credentials remain the only path — which is what a workstation runs.
    state.oauth = totem_gateway::oauth_from_env();
    match state.oauth.as_ref() {
        Some(verifier) => println!("oauth: resource server for {}", verifier.metadata_url()),
        None => println!(
            "oauth: not configured (static bearer credentials only); set \
             TOTEM_OAUTH_ISSUER/_RESOURCE/_REPO/_SCOPE to enable"
        ),
    }

    // Durable grants first (ADV-GATEWAY-012): everything issued through the
    // gateway survives the restart a deploy performs. The bootstrap
    // credential is layered on top so a data directory that has lost its
    // credentials — or a brand-new one — is still reachable.
    let durable = match state.store.credentials().active().await {
        Ok(rows) => match state.tokens.load_from(rows) {
            Ok(0) => 0,
            Ok(loaded) => {
                println!("loaded {loaded} durable credential(s)");
                loaded
            }
            Err(error) => {
                eprintln!("refusing to start: stored credentials are unreadable: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            // Fail closed and loudly: continuing would silently serve a
            // gateway that has forgotten who may call it.
            eprintln!("refusing to start: cannot read stored credentials: {error}");
            std::process::exit(1);
        }
    };

    match bootstrap(&state.tokens) {
        Ok(Some((grant, fingerprint))) => {
            // Persist it like any other grant, so a later `credential list`
            // shows every credential that can reach this gateway rather than
            // all-but-the-bootstrap-one.
            let row = totem_store::CredentialGrantRow {
                fingerprint,
                repo: grant.repo.to_string(),
                scope: grant.scope.to_string(),
                actor: grant.actor.to_string(),
                expires_at: grant.expires_at,
                revoked: false,
            };
            match state.store.credentials().record(&row).await {
                Ok(()) => println!(
                    "registered bootstrap credential: repo {}, scope {}, actor {}",
                    grant.repo, grant.scope, grant.actor
                ),
                Err(error) => {
                    // A revoked bootstrap fingerprint is the interesting case:
                    // someone revoked this credential deliberately, and the
                    // environment still carries it. Refuse rather than quietly
                    // honouring a revoked token.
                    eprintln!("refusing to start: bootstrap credential rejected: {error}");
                    std::process::exit(1);
                }
            }
        }
        // Only warn when the gateway genuinely cannot be reached. Before
        // ADV-GATEWAY-012 an absent bootstrap meant an empty registry; now
        // durable grants may already have filled it, and warning anyway told
        // an operator their credentials were gone when they were not.
        Ok(None) if durable == 0 => eprintln!(
            "warning: no credential is registered, so every request will be refused with 401. \
             Set {TOKEN_VAR}/{REPO_VAR}/{SCOPE_VAR}/{ACTOR_VAR} to seed one."
        ),
        Ok(None) => {}
        Err(error) => {
            eprintln!("refusing to start: {error}");
            std::process::exit(1);
        }
    }

    let app = totem_gateway::authenticated_app(state);

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
