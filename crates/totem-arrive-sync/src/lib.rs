//! Ingestion of an enrolled repo's `/arrive/` artifacts into the Totem
//! landscape graph (docs/solution-intent.md §2.3; ADV-ARRIVE-SYNC-001).
//!
//! `/arrive/` stays authoritative: everything here reads files, parses them,
//! and hands the result to [`totem_store::LandscapeRepository`] — nothing in
//! this crate writes back to the artifact tree
//! (`arrive-sync.yaml`'s invariant).
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use std::path::Path;
//! use surrealdb::engine::local::Db;
//! use totem_store::Store;
//!
//! let store = Store::<Db>::in_memory().await?;
//! store.migrate().await?;
//!
//! let summary =
//!     totem_arrive_sync::sync_repo(&store, Path::new("/path/to/repo/arrive"), "dogfood").await?;
//! # let _ = summary;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use surrealdb::Connection;
use totem_store::{
    AdvanceArtifact, ComponentArtifact, LandscapeSnapshot, OwnerArtifact, RepoArtifact, Store,
    SyncSummary, SystemArtifact,
};

/// Why a repo's `/arrive/` tree could not be ingested.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IngestError {
    /// A file could not be read.
    #[error("reading {path}: {source}")]
    Io {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// A file's YAML (or YAML frontmatter) did not parse.
    #[error("parsing {path}: {source}")]
    Yaml {
        /// The file that failed to parse.
        path: PathBuf,
        /// The underlying parse failure.
        #[source]
        source: serde_yaml::Error,
    },
    /// An advance file had no leading `---`-delimited frontmatter block.
    #[error("{}: expected a leading `---`-delimited YAML frontmatter block", .0.display())]
    MissingFrontmatter(PathBuf),
    /// The store refused the parsed snapshot.
    #[error("the store rejected the sync: {0}")]
    Store(#[from] totem_store::StoreError),
}

fn read(path: &Path) -> Result<String, IngestError> {
    fs::read_to_string(path).map_err(|source| IngestError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_yaml<T: for<'de> Deserialize<'de>>(path: &Path, text: &str) -> Result<T, IngestError> {
    serde_yaml::from_str(text).map_err(|source| IngestError::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

/// Sorted directory entries, so ingestion order (and therefore any tie-break
/// among otherwise-equal artifacts) does not depend on the filesystem's
/// unspecified `read_dir` order.
fn sorted_entries(dir: &Path, extension: &str) -> Result<Vec<PathBuf>, IngestError> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).map_err(|source| IngestError::Io {
        path: dir.to_path_buf(),
        source,
    })? {
        // A per-entry error (permissions, a concurrent delete) is dropped
        // silently by `.ok()` — reported instead, so a partial directory
        // read never produces a snapshot that looks complete but isn't.
        let entry = entry.map_err(|source| IngestError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some(extension) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    registry: RegistryFields,
    systems: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RegistryFields {
    repo_id: String,
    name: String,
    git_repo: String,
}

#[derive(Debug, Deserialize)]
struct SystemFile {
    system: SystemFields,
}

#[derive(Debug, Deserialize)]
struct SystemFields {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct ComponentFile {
    component: ComponentFields,
}

#[derive(Debug, Deserialize)]
struct ComponentFields {
    id: String,
    name: String,
    stage: Option<String>,
    #[serde(default)]
    owners: Vec<OwnerFields>,
}

#[derive(Debug, Deserialize)]
struct OwnerFields {
    team: Option<String>,
    user: Option<String>,
}

/// `{team: "x"}` becomes `team:x`; `{user: "x"}` becomes `user:x`. An owner
/// entry naming neither is dropped rather than guessed at — a landscape
/// owner with no identifiable name is not a fact worth mirroring.
fn owner_artifact(owner: OwnerFields) -> Option<OwnerArtifact> {
    if let Some(team) = owner.team {
        Some(OwnerArtifact {
            id: format!("team:{team}"),
            name: team,
        })
    } else {
        owner.user.map(|user| OwnerArtifact {
            id: format!("user:{user}"),
            name: user,
        })
    }
}

#[derive(Debug, Deserialize)]
struct AdvanceFrontmatter {
    advance: AdvanceFields,
}

#[derive(Debug, Deserialize)]
struct AdvanceFields {
    id: String,
    title: String,
    system: String,
    #[serde(default)]
    components: Vec<String>,
    status: String,
}

/// The `---`-delimited YAML block at the top of an advance file, per
/// `arrive-advance-writing.md`'s frontmatter schema. Markdown horizontal
/// rules (`---`) can appear later in the body; only the first line after the
/// opening delimiter that is itself exactly `---` closes the block, matching
/// the standard frontmatter convention.
fn extract_frontmatter(path: &Path, text: &str) -> Result<String, IngestError> {
    let mut lines = text.lines();
    match lines.next() {
        Some("---") => {}
        _ => return Err(IngestError::MissingFrontmatter(path.to_path_buf())),
    }

    let mut frontmatter = Vec::new();
    for line in lines {
        if line == "---" {
            return Ok(frontmatter.join("\n"));
        }
        frontmatter.push(line);
    }
    Err(IngestError::MissingFrontmatter(path.to_path_buf()))
}

fn read_registry(arrive_root: &Path) -> Result<(RepoArtifact, Vec<String>), IngestError> {
    let path = arrive_root.join("registry.yaml");
    let text = read(&path)?;
    let file: RegistryFile = parse_yaml(&path, &text)?;
    Ok((
        RepoArtifact {
            id: file.registry.repo_id,
            name: file.registry.name,
            git_repo: file.registry.git_repo,
        },
        file.systems,
    ))
}

fn read_system(system_dir: &Path) -> Result<SystemArtifact, IngestError> {
    let path = system_dir.join("system.yaml");
    let text = read(&path)?;
    let file: SystemFile = parse_yaml(&path, &text)?;
    Ok(SystemArtifact {
        id: file.system.id,
        name: file.system.name,
    })
}

fn read_components(
    system_dir: &Path,
    system_id: &str,
) -> Result<Vec<ComponentArtifact>, IngestError> {
    let dir = system_dir.join("components");
    sorted_entries(&dir, "yaml")?
        .into_iter()
        .map(|path| {
            let text = read(&path)?;
            let file: ComponentFile = parse_yaml(&path, &text)?;
            Ok(ComponentArtifact {
                id: file.component.id,
                system: system_id.to_string(),
                name: file.component.name,
                stage: file.component.stage,
                owners: file
                    .component
                    .owners
                    .into_iter()
                    .filter_map(owner_artifact)
                    .collect(),
            })
        })
        .collect()
}

fn read_advances(system_dir: &Path) -> Result<Vec<AdvanceArtifact>, IngestError> {
    let dir = system_dir.join("advances");
    sorted_entries(&dir, "md")?
        .into_iter()
        .map(|path| {
            let text = read(&path)?;
            let frontmatter = extract_frontmatter(&path, &text)?;
            let file: AdvanceFrontmatter = parse_yaml(&path, &frontmatter)?;
            Ok(AdvanceArtifact {
                id: file.advance.id,
                system: file.advance.system,
                title: file.advance.title,
                status: Some(file.advance.status),
                components: file.advance.components,
            })
        })
        .collect()
}

/// Parse one repo's `/arrive/` tree into a snapshot, ready for
/// [`totem_store::LandscapeRepository::sync`].
///
/// `arrive_root` is the `/arrive/` directory itself (e.g. `<repo>/arrive`),
/// not the repo root. Every system named in `registry.yaml`'s `systems` list
/// is read from `<arrive_root>/systems/<id>/`.
pub fn read_repo_artifacts(arrive_root: &Path) -> Result<LandscapeSnapshot, IngestError> {
    let (repo, system_ids) = read_registry(arrive_root)?;

    let mut systems = Vec::with_capacity(system_ids.len());
    let mut components = Vec::new();
    let mut advances = Vec::new();
    for system_id in &system_ids {
        let system_dir = arrive_root.join("systems").join(system_id);
        systems.push(read_system(&system_dir)?);
        components.extend(read_components(&system_dir, system_id)?);
        advances.extend(read_advances(&system_dir)?);
    }

    Ok(LandscapeSnapshot {
        repo,
        systems,
        components,
        advances,
    })
}

/// Parse `arrive_root` and sync the result into `store` in one call — the
/// enroll-time and hook-triggered entry point (`totem enroll`, ADV-CLI-001,
/// will call this; this advance dogfoods it directly against this repo).
///
/// Generic over the store's connection: this crate only ever calls
/// `Store::landscape()`, never touches the connection directly, so it makes
/// no assumption about which engine `store` was built against (the
/// embedded engine every test here uses, or a future server connection).
pub async fn sync_repo<C: Connection>(
    store: &Store<C>,
    arrive_root: &Path,
    source: &str,
) -> Result<SyncSummary, IngestError> {
    let snapshot = read_repo_artifacts(arrive_root)?;
    Ok(store.landscape().sync(&snapshot, source).await?)
}
