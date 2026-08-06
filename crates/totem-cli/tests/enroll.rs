//! `enroll`/`sync` end-to-end against a minimal, synthetic `/arrive/` tree in
//! a temp git repo. Deliberately never run against this actual repo: enroll
//! installs a real git hook, and this suite must not touch the real
//! `.git/hooks` it runs inside of.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use totem_cli::error::CliError;

fn seed_arrive_tree(root: &Path) {
    let arrive = root.join("arrive");
    fs::create_dir_all(arrive.join("systems/demo-system/components")).expect("mkdir components");
    fs::create_dir_all(arrive.join("systems/demo-system/advances")).expect("mkdir advances");

    fs::write(
        arrive.join("registry.yaml"),
        "registry:\n  repo_id: demo/repo\n  name: Demo Repo\nsystems:\n  - demo-system\n",
    )
    .expect("write registry.yaml");
    fs::write(
        arrive.join("systems/demo-system/system.yaml"),
        "system:\n  id: demo-system\n  name: Demo System\n",
    )
    .expect("write system.yaml");
    fs::write(
        arrive.join("systems/demo-system/components/demo.yaml"),
        "component:\n  id: demo\n  name: Demo Component\n  stage: incubating\n",
    )
    .expect("write demo.yaml");
    fs::write(
        arrive.join("systems/demo-system/advances/ADV-DEMO-001.md"),
        concat!(
            "---\n",
            "advance:\n",
            "  id: ADV-DEMO-001\n",
            "  title: Demo advance\n",
            "  system: demo-system\n",
            "  components: [demo]\n",
            "  status: complete\n",
            "---\n\n",
            "body\n",
        ),
    )
    .expect("write ADV-DEMO-001.md");
}

fn init_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(dir.path())
        .status()
        .expect("git init runs");
    assert!(status.success(), "git init must succeed");
    seed_arrive_tree(dir.path());
    dir
}

#[tokio::test]
async fn enrolling_syncs_the_landscape_and_installs_the_hook() {
    let repo = init_repo();

    let outcome = totem_cli::enroll::enroll(repo.path())
        .await
        .expect("enroll succeeds");

    assert_eq!(outcome.sync.systems, 1);
    assert_eq!(outcome.sync.components, 1);
    assert_eq!(outcome.sync.advances, 1);
    assert_eq!(outcome.hooks.len(), 2);

    assert!(repo.path().join(".git/hooks/totem-sync-hook").exists());
    assert!(repo.path().join(".git/hooks/post-commit").exists());
    assert!(repo.path().join(".git/hooks/post-merge").exists());
}

#[tokio::test]
async fn sync_alone_does_not_touch_hooks() {
    let repo = init_repo();

    let summary = totem_cli::enroll::sync(repo.path())
        .await
        .expect("sync succeeds");

    assert_eq!(summary.systems, 1);
    assert_eq!(summary.advances, 1);
    assert!(
        !repo.path().join(".git/hooks/post-commit").exists(),
        "sync alone must not install a hook"
    );
}

#[tokio::test]
async fn enrolling_a_nonexistent_path_fails_plainly() {
    let error = totem_cli::enroll::enroll(Path::new("/nonexistent/totem-cli-fixture"))
        .await
        .expect_err("a nonexistent path is not a git repo");
    assert!(matches!(error, CliError::NotAGitRepo { .. }));
}
