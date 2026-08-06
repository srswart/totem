//! Installs the git hook that keeps an enrolled repo's landscape mirror
//! fresh: every commit or merge re-runs `totem sync`
//! (docs/solution-intent.md §3.3 — "installs the sync hook").
//!
//! Mirrors this repo's own `hooks/platform/install.sh` pattern (idempotent,
//! marker-guarded append) rather than inventing a new one — but the hook
//! body itself cannot be a path into *this* repo's `hooks/` tree, because the
//! repo being enrolled is a different one and does not have it checked in.
//! Instead the body is embedded in the `totem` binary at compile time and
//! materialized into the target repo's own `.git/hooks/` directory at
//! enroll time.

use std::fs;
use std::path::Path;

use crate::error::CliError;

const MARKER: &str = "# totem-sync-hook";
const HOOK_BODY_NAME: &str = "totem-sync-hook";
const HOOK_BODY: &str = include_str!("../hooks/totem-sync-hook.sh");
const HOOKED_EVENTS: [&str; 2] = ["post-commit", "post-merge"];

/// What happened to one hook file during install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookOutcome {
    /// The hook file did not exist; created with the invocation line.
    Created,
    /// The hook file existed without the marker; the invocation line was
    /// appended, and everything already in the file was preserved.
    Appended,
    /// The marker was already present; nothing changed.
    AlreadyInstalled,
}

/// Install the sync hook into `repo_root`'s `.git/hooks/`.
///
/// `repo_root` must be a git worktree root (the directory containing
/// `.git`) — `enroll` resolves it with `git rev-parse --show-toplevel`
/// before calling this. Returns the outcome for each hooked event
/// (`post-commit`, `post-merge`), in that order.
pub fn install(repo_root: &Path) -> Result<Vec<(&'static str, HookOutcome)>, CliError> {
    let hooks_dir = repo_root.join(".git").join("hooks");
    fs::create_dir_all(&hooks_dir).map_err(|source| CliError::Io {
        path: hooks_dir.clone(),
        source,
    })?;

    write_executable(&hooks_dir.join(HOOK_BODY_NAME), HOOK_BODY)?;

    let mut outcomes = Vec::with_capacity(HOOKED_EVENTS.len());
    for event in HOOKED_EVENTS {
        outcomes.push((event, install_invocation(&hooks_dir, event)?));
    }
    Ok(outcomes)
}

fn install_invocation(hooks_dir: &Path, event: &str) -> Result<HookOutcome, CliError> {
    let path = hooks_dir.join(event);
    // `$0` is the invoked hook file's own path, so `dirname "$0"` finds the
    // hook body regardless of where the enrolled repo lives on disk.
    let line = format!("\"$(dirname \"$0\")\"/{HOOK_BODY_NAME} {MARKER}\n");

    if !path.exists() {
        write_executable(&path, &format!("#!/usr/bin/env bash\n{line}"))?;
        return Ok(HookOutcome::Created);
    }

    let existing = fs::read_to_string(&path).map_err(|source| CliError::Io {
        path: path.clone(),
        source,
    })?;
    if existing.contains(MARKER) {
        return Ok(HookOutcome::AlreadyInstalled);
    }

    let separator = if existing.ends_with('\n') { "" } else { "\n" };
    write_executable(&path, &format!("{existing}{separator}{line}"))?;
    Ok(HookOutcome::Appended)
}

#[cfg(unix)]
fn write_executable(path: &Path, content: &str) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, content).map_err(|source| CliError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut perms = fs::metadata(path)
        .map_err(|source| CliError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).map_err(|source| CliError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn write_executable(path: &Path, content: &str) -> Result<(), CliError> {
    fs::write(path, content).map_err(|source| CliError::Io {
        path: path.to_path_buf(),
        source,
    })
}
