//! Actor credential issuance and local storage (docs/solution-intent.md §3.3;
//! ADV-CLI-001). The gateway does not verify these tokens yet — real
//! server-side enforcement of "least-privilege, repo+scope bound" is
//! ADV-GATEWAY-003's job — so these tests cover the shape and local handling
//! this advance actually delivers: well-formed, repo/scope/actor-consistent
//! credentials, stored with restrictive permissions.

use totem_cli::credential::{self, Credential, CredentialError};

#[test]
fn issuing_a_credential_binds_repo_scope_and_actor() {
    let credential = credential::issue("srswart/totem", "project:srswart/totem", "ada")
        .expect("a consistent repo/scope pair issues");

    assert_eq!(credential.repo, "srswart/totem");
    assert_eq!(credential.scope, "project:srswart/totem");
    assert_eq!(credential.actor, "ada");
    assert!(
        !credential.token.is_empty(),
        "a credential must carry a non-empty token"
    );
}

#[test]
fn two_issued_credentials_never_share_a_token() {
    let first = credential::issue("srswart/totem", "platform", "ada").expect("issues");
    let second = credential::issue("srswart/totem", "platform", "ada").expect("issues");
    assert_ne!(first.token, second.token);
}

#[test]
fn a_project_scope_naming_a_different_repo_is_refused() {
    let error = credential::issue("srswart/totem", "project:someone-else/other-repo", "ada")
        .expect_err("mismatched repo/scope must be refused");
    assert!(matches!(error, CredentialError::ScopeMismatch { .. }), "{error:?}");
}

#[test]
fn an_actor_scope_naming_a_different_actor_is_refused() {
    let error = credential::issue("srswart/totem", "actor:grace", "ada")
        .expect_err("a credential cannot claim another actor's private scope");
    assert!(matches!(error, CredentialError::ScopeMismatch { .. }), "{error:?}");
}

#[test]
fn an_unparsable_scope_is_refused() {
    let error = credential::issue("srswart/totem", "not-a-scope", "ada")
        .expect_err("an invalid scope string must be refused");
    assert!(matches!(error, CredentialError::InvalidScope { .. }), "{error:?}");
}

#[test]
fn an_empty_actor_is_refused() {
    let error = credential::issue("srswart/totem", "platform", "")
        .expect_err("an empty actor id must be refused");
    assert!(matches!(error, CredentialError::InvalidActor { .. }), "{error:?}");
}

#[test]
fn storing_then_loading_round_trips_and_appends() {
    let dir = tempfile::tempdir().expect("temp dir creates");
    let path = credential::default_store_path(dir.path());

    let ada = credential::issue("srswart/totem", "actor:ada", "ada").expect("issues");
    credential::store(&path, &ada).expect("stores");

    let grace = credential::issue("srswart/totem", "actor:grace", "grace").expect("issues");
    credential::store(&path, &grace).expect("stores");

    let loaded: Vec<Credential> = credential::load(&path).expect("loads");
    assert_eq!(loaded, vec![ada, grace]);
}

#[test]
fn loading_a_credential_store_that_does_not_exist_yet_is_an_empty_list() {
    let dir = tempfile::tempdir().expect("temp dir creates");
    let path = credential::default_store_path(dir.path());
    assert_eq!(credential::load(&path).expect("loads"), Vec::new());
}

#[cfg(unix)]
#[test]
fn the_credential_store_file_is_only_readable_by_its_owner() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("temp dir creates");
    let path = credential::default_store_path(dir.path());
    let ada = credential::issue("srswart/totem", "actor:ada", "ada").expect("issues");
    credential::store(&path, &ada).expect("stores");

    let mode = std::fs::metadata(&path)
        .expect("file exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "expected owner-only rw, got {mode:o}");
}
