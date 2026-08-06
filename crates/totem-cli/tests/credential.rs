//! Local credential store tests: issuance, listing, revocation, and file
//! permissions (`risk_flags: ["auth"]` — credential handling on developer
//! machines is this advance's named risk).

use totem_cli::credential::CredentialStore;
use totem_cli::error::CliError;
use totem_core::{ActorId, RepoId, Scope};

fn store() -> (tempfile::TempDir, CredentialStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = CredentialStore::at(dir.path().join("credentials.json"));
    (dir, store)
}

fn repo() -> RepoId {
    RepoId::new("srswart/totem").expect("valid repo id")
}

fn actor() -> ActorId {
    ActorId::new("ada").expect("valid actor id")
}

#[test]
fn issuing_a_credential_persists_it_and_returns_a_unique_secret() {
    let (_dir, store) = store();

    let first = store
        .issue(repo(), Scope::Actor(actor()), actor())
        .expect("issues a credential");
    let second = store
        .issue(repo(), Scope::Actor(actor()), actor())
        .expect("issues a second credential");

    assert_ne!(first.id, second.id);
    assert_ne!(
        first.secret, second.secret,
        "two issued credentials must never share a secret"
    );

    let listed = store.list().expect("lists credentials");
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().any(|c| c.id == first.id));
    assert!(listed.iter().any(|c| c.id == second.id));
}

#[test]
fn revoking_a_credential_removes_it_and_unknown_ids_error() {
    let (_dir, store) = store();
    let credential = store
        .issue(repo(), Scope::Platform, actor())
        .expect("issues a credential");

    store.revoke(credential.id).expect("revokes");
    assert!(
        store.list().expect("lists credentials").is_empty(),
        "the revoked credential must be gone"
    );

    let error = store
        .revoke(credential.id)
        .expect_err("revoking an already-revoked id is an error, not a silent no-op");
    assert!(matches!(error, CliError::CredentialNotFound(_)));
}

#[test]
fn a_credential_is_bound_to_the_repo_and_scope_it_was_issued_for() {
    let (_dir, store) = store();
    let credential = store
        .issue(repo(), Scope::Project(repo()), actor())
        .expect("issues a credential");

    assert_eq!(credential.repo, repo());
    assert_eq!(credential.scope, Scope::Project(repo()));
    assert_eq!(credential.actor, actor());
}

#[cfg(unix)]
#[test]
fn the_credential_file_is_not_group_or_world_readable() {
    use std::os::unix::fs::PermissionsExt;

    let (dir, store) = store();
    store
        .issue(repo(), Scope::Platform, actor())
        .expect("issues a credential");

    let path = dir.path().join("credentials.json");
    let mode = std::fs::metadata(&path)
        .expect("reads file metadata")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o077,
        0,
        "credentials.json must not be group- or world-readable"
    );
}
