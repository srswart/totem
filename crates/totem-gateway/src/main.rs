//! The gateway binary: an embedded, in-memory store behind the authenticated
//! application — REST plus MCP over streamable HTTP, every route behind a
//! bearer credential (ADV-GATEWAY-003).
//!
//! In-memory and non-persistent on purpose, for now: production deployment
//! topology (embedded vs. server SurrealDB, where state survives a restart)
//! is an open question (docs/solution-intent.md §9), and this advance's scope
//! is the API surface, not that decision. Whoever resolves the topology
//! question wires `Store::from_connection` onto a durable engine in
//! [`AppState::in_memory`]'s place — and gives the credential registry the
//! same durability, since it is process-local for the same reason.
//!
//! # The bootstrap credential
//!
//! The registry starts empty, and an empty registry refuses every request.
//! That is deliberate: a gateway that served unauthenticated callers whenever
//! it had no credentials configured would fail open exactly when it is least
//! configured. One credential can be seeded from the environment to get an
//! operator in; issuing the rest is the console's and CLI's job
//! (ADV-CONSOLE-002, ADV-CLI-001).

use std::env;

use totem_gateway::{AppState, AuthError, TokenGrant, TokenRegistry};

const TOKEN_VAR: &str = "TOTEM_BOOTSTRAP_TOKEN";
const REPO_VAR: &str = "TOTEM_BOOTSTRAP_REPO";
const SCOPE_VAR: &str = "TOTEM_BOOTSTRAP_SCOPE";
const ACTOR_VAR: &str = "TOTEM_BOOTSTRAP_ACTOR";

/// Register the credential named by the environment, if one is named.
///
/// Takes the token text rather than issuing one so an operator can hand the
/// gateway a credential `totem credential issue` already produced
/// (ADV-CLI-001) — the two describe the same grant.
///
/// Returns the grant that was registered, or `None` when no bootstrap
/// credential was configured. A *partially* configured one is an error, not a
/// silent skip: half-set variables mean someone intended to authenticate.
fn bootstrap(tokens: &TokenRegistry) -> Result<Option<TokenGrant>, AuthError> {
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

    Ok(Some(grant))
}

#[tokio::main]
async fn main() {
    let state = AppState::in_memory()
        .await
        .expect("the embedded engine connects and migrations apply");

    match bootstrap(&state.tokens) {
        Ok(Some(grant)) => println!(
            "registered bootstrap credential: repo {}, scope {}, actor {}",
            grant.repo, grant.scope, grant.actor
        ),
        Ok(None) => eprintln!(
            "warning: no credential is registered, so every request will be refused with 401. \
             Set {TOKEN_VAR}/{REPO_VAR}/{SCOPE_VAR}/{ACTOR_VAR} to seed one."
        ),
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
