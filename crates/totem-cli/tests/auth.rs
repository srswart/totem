//! The CLI presents a credential to the gateway (ADV-CLI-002).
//!
//! `enroll.rs` sent no `Authorization` header at all: every demo worked
//! because the gateway was an unauthenticated loopback composition, and every
//! call against the deployed gateway returned 401. This is the third client
//! built against a server that never refused anything — the MCP connector and
//! the console were the others.

use std::path::PathBuf;

use totem_cli::auth::{CredentialSource, ResolveError, resolve_token};

fn store_with(token: &str, repo: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("totem-cli-auth-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("credentials.json");
    let credential = serde_json::json!([{
        "token": token,
        "repo": repo,
        "scope": format!("project:{repo}"),
        "actor": "ada",
        "issued_at": "2026-08-07T00:00:00Z",
    }]);
    std::fs::write(&path, credential.to_string()).expect("write store");
    path
}

#[test]
fn an_explicit_flag_wins_over_everything() {
    let store = store_with("from-store", "srswart/totem");

    let resolved = resolve_token(
        Some("from-flag"),
        Some("from-env"),
        &store,
        "srswart/totem",
    )
    .expect("resolves");

    assert_eq!(resolved.token, "from-flag");
    assert_eq!(resolved.source, CredentialSource::Flag);
}

#[test]
fn the_environment_wins_over_the_store() {
    let store = store_with("from-store", "srswart/totem");

    let resolved =
        resolve_token(None, Some("from-env"), &store, "srswart/totem").expect("resolves");

    assert_eq!(resolved.token, "from-env");
    assert_eq!(resolved.source, CredentialSource::Environment);
}

#[test]
fn the_store_supplies_a_credential_matching_the_repo() {
    let store = store_with("from-store", "srswart/totem");

    let resolved = resolve_token(None, None, &store, "srswart/totem").expect("resolves");

    assert_eq!(resolved.token, "from-store");
    assert_eq!(resolved.source, CredentialSource::Store);
}

#[test]
fn a_credential_for_another_repo_is_not_used() {
    // Silently presenting a credential bound to a different repo would earn a
    // confusing 403 from the gateway instead of an actionable local error.
    let store = store_with("from-store", "someone/else");

    let outcome = resolve_token(None, None, &store, "srswart/totem");

    assert!(
        matches!(outcome, Err(ResolveError::NoCredential { .. })),
        "a credential bound to another repo must not be presented: {outcome:?}"
    );
}

#[test]
fn no_credential_fails_loudly_and_says_how_to_get_one() {
    // The failure this advance most needs to prevent: a CLI that quietly
    // proceeds unauthenticated works against a loopback gateway and fails
    // confusingly against the real one.
    let store = store_with("irrelevant", "someone/else");

    let error = resolve_token(None, None, &store, "srswart/totem")
        .expect_err("must refuse rather than proceed anonymously");
    let message = error.to_string();

    for expected in ["--token", "TOTEM_TOKEN", "credentials.json"] {
        assert!(
            message.contains(expected),
            "the error must name the resolution order so a first-time user \
             knows what to do; missing {expected:?} in: {message}"
        );
    }
}

#[test]
fn a_resolved_credential_never_renders_its_token() {
    let store = store_with("super-secret-token", "srswart/totem");
    let resolved = resolve_token(None, None, &store, "srswart/totem").expect("resolves");

    let rendered = format!("{resolved:?}");
    assert!(
        !rendered.contains("super-secret-token"),
        "Debug output must not leak the token — it reaches logs and CI \
         transcripts: {rendered}"
    );
}
