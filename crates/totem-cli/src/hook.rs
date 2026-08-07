//! `totem enroll`'s sync-hook install: a `post-commit` git hook that
//! re-invokes `totem enroll`, so an enrolled repo's landscape stays synced
//! after every commit without a human re-running the command
//! (docs/solution-intent.md §3.3; ADV-CLI-001).
//!
//! Never overwrites a `post-commit` hook it did not itself install — a
//! developer's own hook is their own work, not this crate's to clobber.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

const MARKER: &str = "# totem-enroll-hook (ADV-CLI-001): re-installed by `totem enroll`";
const HOOK_NAME: &str = "post-commit";

/// Why the sync hook could not be installed.
#[derive(Debug, Error)]
pub enum HookError {
    /// `repo_root` has no `.git/hooks` directory.
    #[error("{0} is not a git repository (no .git/hooks directory)")]
    NotAGitRepo(PathBuf),
    /// A `post-commit` hook exists that this crate did not install.
    #[error(
        "{0} already has a post-commit hook that totem did not install; remove or merge it by hand"
    )]
    ForeignHookExists(PathBuf),
    /// Reading or writing the hook file failed.
    #[error("{path}: {source}")]
    Io {
        /// The path that could not be read or written.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: io::Error,
    },
}

/// The hook body.
///
/// Deliberately carries **no credential** (ADV-CLI-002): `totem enroll`
/// resolves one from `TOTEM_TOKEN` or the local store at run time. A token
/// written into this script would sit in the repo's `.git/hooks`, survive in
/// backups, and be trivially readable — and the hook runs unattended, which
/// is exactly when a leaked credential goes unnoticed.
fn script(gateway_url: &str) -> String {
    format!(
        "#!/bin/sh\n\
         {MARKER}\n\
         # No credential here on purpose: `totem enroll` reads TOTEM_TOKEN or\n\
         # the local credential store. See ADV-CLI-002.\n\
         exec totem enroll --repo-root \"$(git rev-parse --show-toplevel)\" \
         --gateway-url \"{gateway_url}\" --source hook:post-commit\n"
    )
}

/// Install (or idempotently re-install, e.g. after a gateway URL change) the
/// `post-commit` sync hook into `repo_root`'s `.git/hooks/`.
pub fn install(repo_root: &Path, gateway_url: &str) -> Result<PathBuf, HookError> {
    let hooks_dir = repo_root.join(".git").join("hooks");
    if !hooks_dir.is_dir() {
        return Err(HookError::NotAGitRepo(repo_root.to_path_buf()));
    }
    let hook_path = hooks_dir.join(HOOK_NAME);

    if hook_path.exists() {
        let existing = fs::read_to_string(&hook_path).map_err(|source| HookError::Io {
            path: hook_path.clone(),
            source,
        })?;
        if !existing.contains(MARKER) {
            return Err(HookError::ForeignHookExists(hook_path));
        }
    }

    fs::write(&hook_path, script(gateway_url)).map_err(|source| HookError::Io {
        path: hook_path.clone(),
        source,
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).map_err(|source| {
            HookError::Io {
                path: hook_path.clone(),
                source,
            }
        })?;
    }

    Ok(hook_path)
}
