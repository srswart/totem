//! Local, CLI-issued scoped credentials (docs/solution-intent.md §3.3: "An
//! actor... enrolls by obtaining a scoped credential; cloud agents get
//! least-privilege tokens bound to repo + scope").
//!
//! No gateway endpoint verifies these yet: `totem-gateway` has no auth layer
//! at all today (`SaveRequest`/`RecallRequest` trust a caller-supplied
//! `author` as-is), and ADV-GATEWAY-003 ("Streamable-HTTP MCP + auth for
//! cloud agents", still `planned`) owns server-side token verification and
//! revocation. What this module provides is the *local* half: minting an
//! opaque, repo+scope-bound secret and keeping a record of what was issued,
//! so `totem credential revoke` has a real registry to act on once a
//! verification path exists to consult it.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use totem_core::{ActorId, RepoId, Scope};
use uuid::Uuid;

use crate::error::CliError;

/// One issued credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    /// Unique id, used to revoke this credential without naming its secret.
    pub id: Uuid,
    /// The repo this credential is bound to.
    pub repo: RepoId,
    /// The scope this credential is bound to.
    pub scope: Scope,
    /// The actor this credential was issued to.
    pub actor: ActorId,
    /// The opaque bearer secret. Printed once at issuance, never logged.
    pub secret: String,
    /// When this credential was issued.
    pub issued_at: DateTime<Utc>,
}

/// A 256-bit opaque secret: two v4 UUIDs' randomness concatenated, so this
/// module needs no new dependency beyond the `uuid` crate every other crate
/// in the workspace already pulls in for its CSPRNG-backed `new_v4`.
fn generate_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CredentialFile {
    #[serde(default)]
    credentials: Vec<Credential>,
}

/// The local credential store: one JSON file, `$TOTEM_HOME/credentials.json`
/// (`$TOTEM_HOME` defaults to `$HOME/.totem`).
#[derive(Debug)]
pub struct CredentialStore {
    path: PathBuf,
}

impl CredentialStore {
    /// Open the store at the default location, honouring `$TOTEM_HOME`.
    pub fn open_default() -> Result<Self, CliError> {
        Ok(Self::at(totem_home()?.join("credentials.json")))
    }

    /// Open the store at an explicit path. Tests use this so they never
    /// touch the real `$HOME`.
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self) -> Result<CredentialFile, CliError> {
        if !self.path.exists() {
            return Ok(CredentialFile::default());
        }
        let text = fs::read_to_string(&self.path).map_err(|source| CliError::Io {
            path: self.path.clone(),
            source,
        })?;
        serde_json::from_str(&text).map_err(|source| CliError::CredentialFile {
            path: self.path.clone(),
            source,
        })
    }

    fn write(&self, file: &CredentialFile) -> Result<(), CliError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| CliError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let text =
            serde_json::to_string_pretty(file).map_err(|source| CliError::CredentialFile {
                path: self.path.clone(),
                source,
            })?;
        fs::write(&self.path, text).map_err(|source| CliError::Io {
            path: self.path.clone(),
            source,
        })?;
        restrict_permissions(&self.path)
    }

    /// Mint and persist a new credential bound to `repo` + `scope` for
    /// `actor`.
    pub fn issue(
        &self,
        repo: RepoId,
        scope: Scope,
        actor: ActorId,
    ) -> Result<Credential, CliError> {
        let mut file = self.read()?;
        let credential = Credential {
            id: Uuid::new_v4(),
            repo,
            scope,
            actor,
            secret: generate_secret(),
            issued_at: Utc::now(),
        };
        file.credentials.push(credential.clone());
        self.write(&file)?;
        Ok(credential)
    }

    /// Every credential issued to date. Secrets are included — this is a
    /// local, single-user file, not a network response.
    pub fn list(&self) -> Result<Vec<Credential>, CliError> {
        Ok(self.read()?.credentials)
    }

    /// Remove the credential with the given id. Errors when no such
    /// credential exists, so a mistyped id is not silently a no-op.
    pub fn revoke(&self, id: Uuid) -> Result<(), CliError> {
        let mut file = self.read()?;
        let before = file.credentials.len();
        file.credentials.retain(|credential| credential.id != id);
        if file.credentials.len() == before {
            return Err(CliError::CredentialNotFound(id.to_string()));
        }
        self.write(&file)
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = fs::metadata(path)
        .map_err(|source| CliError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms).map_err(|source| CliError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), CliError> {
    Ok(())
}

fn totem_home() -> Result<PathBuf, CliError> {
    if let Ok(dir) = std::env::var("TOTEM_HOME") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").map_err(|_| CliError::NoHomeDir)?;
    Ok(PathBuf::from(home).join(".totem"))
}
