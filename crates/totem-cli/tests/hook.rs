//! Sync hook install (`totem enroll`'s "install the sync hook",
//! docs/solution-intent.md §3.3; ADV-CLI-001): a `post-commit` hook that
//! re-runs `totem enroll` so later `/arrive/` changes sync automatically.

use std::fs;

use totem_cli::hook::{self, HookError};

fn init_git_hooks_dir(repo_root: &std::path::Path) {
    fs::create_dir_all(repo_root.join(".git").join("hooks")).expect("creates .git/hooks");
}

#[test]
fn installing_writes_an_executable_hook_naming_the_gateway() {
    let dir = tempfile::tempdir().expect("temp dir creates");
    init_git_hooks_dir(dir.path());

    let hook_path = hook::install(dir.path(), "http://127.0.0.1:8787").expect("installs");
    assert_eq!(
        hook_path,
        dir.path().join(".git").join("hooks").join("post-commit")
    );

    let contents = fs::read_to_string(&hook_path).expect("hook file reads");
    assert!(contents.contains("http://127.0.0.1:8787"));
    assert!(contents.contains("totem enroll"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&hook_path)
            .expect("hook exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "hook must be executable, got {mode:o}");
    }
}

#[test]
fn installing_twice_is_idempotent_and_updates_the_gateway_url() {
    let dir = tempfile::tempdir().expect("temp dir creates");
    init_git_hooks_dir(dir.path());

    hook::install(dir.path(), "http://127.0.0.1:8787").expect("first install");
    hook::install(dir.path(), "http://127.0.0.1:9999").expect("second install");

    let hook_path = dir.path().join(".git").join("hooks").join("post-commit");
    let contents = fs::read_to_string(&hook_path).expect("hook file reads");
    assert!(contents.contains("http://127.0.0.1:9999"));
    assert!(!contents.contains("http://127.0.0.1:8787"));
}

#[test]
fn installing_without_a_git_directory_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir creates");
    // No .git/hooks created.
    let error = hook::install(dir.path(), "http://127.0.0.1:8787")
        .expect_err("a non-git directory must be refused");
    assert!(matches!(error, HookError::NotAGitRepo(_)), "{error:?}");
}

#[test]
fn a_foreign_post_commit_hook_is_never_overwritten() {
    let dir = tempfile::tempdir().expect("temp dir creates");
    init_git_hooks_dir(dir.path());
    let hook_path = dir.path().join(".git").join("hooks").join("post-commit");
    fs::write(&hook_path, "#!/bin/sh\necho a developer's own hook\n").expect("writes");

    let error = hook::install(dir.path(), "http://127.0.0.1:8787")
        .expect_err("a foreign hook must be refused, not clobbered");
    assert!(
        matches!(error, HookError::ForeignHookExists(_)),
        "{error:?}"
    );

    let contents = fs::read_to_string(&hook_path).expect("hook file reads");
    assert!(
        contents.contains("a developer's own hook"),
        "foreign hook was overwritten"
    );
}
