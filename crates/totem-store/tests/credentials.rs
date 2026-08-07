//! Credential grants survive a restart, and revocation is permanent
//! (ADV-GATEWAY-012).
//!
//! The registry that verifies bearer credentials was in-memory until this
//! advance: every grant except the env-seeded bootstrap vanished when the
//! gateway restarted, which a deployed instance does on every deploy.

mod common;

use common::store;
use totem_store::{CredentialGrantRow, StoreError};

fn grant(fingerprint: &str, actor: &str) -> CredentialGrantRow {
    CredentialGrantRow {
        fingerprint: fingerprint.to_string(),
        repo: common::REPO.to_string(),
        scope: format!("actor:{actor}"),
        actor: actor.to_string(),
        expires_at: None,
        revoked: false,
    }
}

#[tokio::test]
async fn a_stored_grant_is_readable_back() {
    let store = store().await;
    store
        .credentials()
        .record(&grant("fp-ada", "ada"))
        .await
        .expect("record");

    let all = store.credentials().active().await.expect("read");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].actor, "ada");
    assert_eq!(all[0].scope, "actor:ada");
}

#[tokio::test]
async fn revocation_removes_a_grant_from_the_active_set() {
    let store = store().await;
    let credentials = store.credentials();
    credentials
        .record(&grant("fp-ada", "ada"))
        .await
        .expect("record");
    credentials
        .record(&grant("fp-grace", "grace"))
        .await
        .expect("record");

    credentials.revoke("fp-ada").await.expect("revoke");

    let active = credentials.active().await.expect("read");
    assert_eq!(active.len(), 1, "the revoked grant must not be active");
    assert_eq!(active[0].actor, "grace");
}

#[tokio::test]
async fn revoking_an_unknown_fingerprint_is_refused_not_silently_ignored() {
    let store = store().await;

    let outcome = store.credentials().revoke("fp-never-issued").await;

    assert!(
        matches!(outcome, Err(StoreError::CredentialNotFound(_))),
        "revoking an unknown credential must report it — a revocation that \
         silently does nothing reads as success to an operator racing an \
         incident, got {outcome:?}"
    );
}

#[tokio::test]
async fn a_revoked_grant_stays_revoked_and_is_never_resurrected() {
    let store = store().await;
    let credentials = store.credentials();
    credentials
        .record(&grant("fp-ada", "ada"))
        .await
        .expect("record");
    credentials.revoke("fp-ada").await.expect("revoke");

    // Re-recording the same fingerprint must not undo a revocation: an
    // attacker (or a careless re-issue) replaying an old fingerprint cannot
    // bring a revoked credential back.
    let replay = credentials.record(&grant("fp-ada", "ada")).await;
    assert!(
        replay.is_err(),
        "re-recording a revoked fingerprint must be refused, got {replay:?}"
    );

    let active = credentials.active().await.expect("read");
    assert!(
        active.is_empty(),
        "revoked credential came back: {active:?}"
    );
}

#[tokio::test]
async fn the_store_never_holds_token_text() {
    let store = store().await;
    store
        .credentials()
        .record(&grant("fp-only-a-hash", "ada"))
        .await
        .expect("record");

    // The row type has no field that could carry a token: this is a
    // compile-time property, asserted here so a future field addition that
    // introduces one fails a test rather than passing review.
    let stored = store.credentials().active().await.expect("read");
    let rendered = format!("{:?}", stored[0]);
    assert!(
        !rendered.contains("token"),
        "a credential row must not carry token text: {rendered}"
    );
}
