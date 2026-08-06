//! Hook installer tests: installing into a fresh repo, idempotency, and
//! preserving an existing hook file's content when appending
//! (`hooks/platform/install.sh`'s pattern, applied here to a repo that does
//! not have this crate's own `hooks/` tree checked in).

use std::fs;
use std::process::Command;

use tempfile::TempDir;
use totem_cli::hook::{self, HookOutcome};

fn init_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(dir.path())
        .status()
        .expect("git init runs");
    assert!(status.success(), "git init must succeed");
    dir
}

#[test]
fn installing_into_a_fresh_repo_creates_both_hook_files() {
    let repo = init_repo();

    let outcomes = hook::install(repo.path()).expect("install succeeds");

    assert_eq!(outcomes.len(), 2);
    assert!(
        outcomes
            .iter()
            .all(|(_, outcome)| *outcome == HookOutcome::Created)
    );

    let body = repo.path().join(".git/hooks/totem-sync-hook");
    assert!(body.exists(), "the hook body must be materialized");

    let post_commit = fs::read_to_string(repo.path().join(".git/hooks/post-commit"))
        .expect("post-commit was created");
    assert!(post_commit.contains("# totem-sync-hook"));
    assert!(post_commit.starts_with("#!/usr/bin/env bash"));

    let post_merge = fs::read_to_string(repo.path().join(".git/hooks/post-merge"))
        .expect("post-merge was created");
    assert!(post_merge.contains("# totem-sync-hook"));
}

#[test]
fn installing_twice_is_idempotent() {
    let repo = init_repo();
    hook::install(repo.path()).expect("first install succeeds");
    let before =
        fs::read_to_string(repo.path().join(".git/hooks/post-commit")).expect("reads once");

    let outcomes = hook::install(repo.path()).expect("second install succeeds");
    assert!(
        outcomes
            .iter()
            .all(|(_, outcome)| *outcome == HookOutcome::AlreadyInstalled)
    );

    let after =
        fs::read_to_string(repo.path().join(".git/hooks/post-commit")).expect("reads twice");
    assert_eq!(
        before, after,
        "a repeated install must not duplicate the invocation line"
    );
}

#[test]
fn installing_into_an_existing_hook_file_appends_without_clobbering_it() {
    let repo = init_repo();
    let post_commit = repo.path().join(".git/hooks/post-commit");
    fs::create_dir_all(post_commit.parent().expect("hooks dir has a parent"))
        .expect("mkdir .git/hooks");
    fs::write(&post_commit, "#!/usr/bin/env bash\necho existing-hook\n")
        .expect("seed an existing hook");

    let outcomes = hook::install(repo.path()).expect("install succeeds");
    assert!(
        outcomes
            .iter()
            .any(|(event, outcome)| *event == "post-commit" && *outcome == HookOutcome::Appended)
    );

    let content = fs::read_to_string(&post_commit).expect("reads the hook");
    assert!(
        content.contains("echo existing-hook"),
        "an existing hook body must survive the install"
    );
    assert!(content.contains("# totem-sync-hook"));
}
