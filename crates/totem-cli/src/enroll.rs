//! `totem enroll`/`totem sync`: register a repo with Totem, run its
//! landscape sync, and (enroll only) install the git hook that keeps future
//! syncs automatic (docs/solution-intent.md §3.3).
//!
//! Deployment topology is still an open question
//! (docs/solution-intent.md §9) — `totem-gateway`'s own binary connects to a
//! fresh embedded in-memory instance on every start for the same reason.
//! `enroll`/`sync` do the same here, so there is no durable "registration"
//! record distinct from a sync: each invocation stands up its own throwaway
//! store, and the first successful sync *is* the registration event. Once a
//! durable store connection exists, upgrading these functions to use it is a
//! change to *where* they connect, not to the ingestion logic itself, since
//! both already call the real `totem_arrive_sync::sync_repo` /
//! `totem_store::Store` API `ADV-ARRIVE-SYNC-001` wired against this repo.

use std::path::{Path, PathBuf};

use totem_store::{Store, SyncSummary};

use crate::error::CliError;
use crate::hook::{self, HookOutcome};

/// What `enroll` did.
#[derive(Debug)]
pub struct EnrollOutcome {
    /// What the landscape sync wrote.
    pub sync: SyncSummary,
    /// What happened to each hook file, in the order `hook::install` tried
    /// them.
    pub hooks: Vec<(&'static str, HookOutcome)>,
}

/// Find the git worktree root containing `path`, the same way the installed
/// hook itself does (`git rev-parse --show-toplevel`), so enrollment and the
/// hook it installs always agree on where `.git/hooks/` and `/arrive/` live.
fn git_root(path: &Path) -> Result<PathBuf, CliError> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .map_err(|source| CliError::NotAGitRepo {
            path: path.to_path_buf(),
            source,
        })?;
    if !output.status.success() {
        return Err(CliError::NotAGitRepo {
            path: path.to_path_buf(),
            source: std::io::Error::other(String::from_utf8_lossy(&output.stderr).into_owned()),
        });
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

async fn sync_at(root: &Path, source: &str) -> Result<SyncSummary, CliError> {
    let arrive_root = root.join("arrive");
    let store = Store::in_memory().await?;
    store.migrate().await?;
    Ok(totem_arrive_sync::sync_repo(&store, &arrive_root, source).await?)
}

/// Run the landscape sync only (no hook install) — what `totem sync` and the
/// hook it installs both call.
pub async fn sync(path: &Path) -> Result<SyncSummary, CliError> {
    let root = git_root(path)?;
    sync_at(&root, "cli:sync").await
}

/// Register the repo (its first sync) and install the sync hook.
pub async fn enroll(path: &Path) -> Result<EnrollOutcome, CliError> {
    let root = git_root(path)?;
    let sync = sync_at(&root, "cli:enroll").await?;
    let hooks = hook::install(&root)?;
    Ok(EnrollOutcome { sync, hooks })
}
