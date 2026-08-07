//! Landscape ingestion: the `repo -> system -> component` / `advance` mirror
//! of an enrolled repo's `/arrive/` artifacts (docs/solution-intent.md §2.3),
//! and the sync provenance recorded for every ingestion run.
//!
//! `totem-arrive-sync` parses artifact files into the structures here; this
//! module is the only place that writes them into SurrealDB, the same split
//! [`MemoryRepository`](crate::MemoryRepository) draws between a domain
//! record and its persistence.
//!
//! Re-running a sync is idempotent: every entity is addressed by a
//! deterministic id derived from its artifact id, so ingesting the same
//! `/arrive/` tree twice converges rather than duplicating rows. The
//! `owned_by` and `impacts` edges touched by a sync are replaced wholesale —
//! the landscape mirror is disposable and always re-derivable from
//! `/arrive/` (the advance's own Risk + Rollback), so a stale edge from a
//! component that dropped an owner, or an advance that no longer impacts a
//! component, must not survive a re-sync.

use std::pin::Pin;

use chrono::{DateTime, Utc};
use futures::stream::{Stream, StreamExt, select_all};
use serde::{Deserialize, Serialize};
use surrealdb::types::{Number, Object, RecordId, RecordIdKey, SurrealValue, Value};
use surrealdb::{Connection, Surreal};

use crate::error::{StoreError, StoreResult};
use crate::row::{self, malformed};

const REPO_TABLE: &str = "repo";
const SYSTEM_TABLE: &str = "system";
const COMPONENT_TABLE: &str = "component";
const ADVANCE_TABLE: &str = "advance";
const ACTOR_TABLE: &str = "actor";
const SYNC_RUN_TABLE: &str = "sync_run";

/// One repo, as the landscape mirrors it (`arrive/registry.yaml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoArtifact {
    /// The ARRIVE registry id (`registry.repo_id`), e.g. `"058-totem"` — what
    /// this repo's `repo` record is keyed by, and what
    /// `GET /landscape/:repo` and `totem_landscape` take as their `repo`
    /// parameter. Left unchanged by ADV-GATEWAY-009 so existing landscape
    /// readers (the console, this repo's own dogfood tests) keep working.
    pub id: String,
    /// Its display name.
    pub name: String,
    /// The `owner/name` GitHub identity (`registry.git_repo`) — the same id
    /// space a gateway credential's `repo` binding speaks (`totem-gateway`'s
    /// `TokenGrant::repo`). A credential's binding is checked against this
    /// field, not against [`RepoArtifact::id`]: the two are different id
    /// spaces (ADV-GATEWAY-003's disclosed residual), and this field is
    /// ADV-GATEWAY-009's unification of them.
    pub git_repo: String,
}

/// One system within a repo (`arrive/systems/<id>/system.yaml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemArtifact {
    /// The system id.
    pub id: String,
    /// Its display name.
    pub name: String,
}

/// One owner reference on a component (`component.owners[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerArtifact {
    /// A stable id for the owner, e.g. `team:058-totem`.
    pub id: String,
    /// The owner's plain name, e.g. `058-totem`.
    pub name: String,
}

/// One component within a system (`arrive/systems/<id>/components/*.yaml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentArtifact {
    /// The component id, unique within its system.
    pub id: String,
    /// The system it belongs to.
    pub system: String,
    /// Its display name.
    pub name: String,
    /// Its lifecycle stage (`incubating` / `candidate` / `resident`), if set.
    pub stage: Option<String>,
    /// Who owns it.
    pub owners: Vec<OwnerArtifact>,
}

impl ComponentArtifact {
    /// The record key landscape rows use: namespaced by system, so two
    /// systems may each declare a component of the same short name without
    /// colliding. The short id round-trips through the stored
    /// `component_id` field rather than being parsed back out of this key.
    fn key(&self) -> String {
        component_key(&self.system, &self.id)
    }
}

fn component_key(system: &str, component: &str) -> String {
    format!("{system}__{component}")
}

/// One advance within a system (`arrive/systems/<id>/advances/*.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvanceArtifact {
    /// The advance id (`ADV-<COMPONENT>-<SEQ>`), already globally unique by
    /// ARRIVE convention, so it is used as the record key directly.
    pub id: String,
    /// The system it belongs to.
    pub system: String,
    /// Its title.
    pub title: String,
    /// Its frontmatter status (`planned` / `in_progress` / `complete` /
    /// `cancelled`), if set.
    pub status: Option<String>,
    /// The short component ids it impacts (`advance.components`), resolved
    /// against `system` when the `impacts` edge is written.
    pub components: Vec<String>,
}

/// Everything ingested from one repo's `/arrive/` tree in a single run.
///
/// `Serialize`/`Deserialize` on this and its fields (`RepoArtifact`,
/// `SystemArtifact`, `ComponentArtifact`, `OwnerArtifact`, `AdvanceArtifact`)
/// let a snapshot cross a process boundary as JSON — the gateway's `POST
/// /enroll` (ADV-CLI-001) is the first caller: `totem-cli` parses `/arrive/`
/// locally via `totem-arrive-sync` and sends the resulting snapshot to a
/// running gateway rather than opening its own store connection, since the
/// gateway (not the CLI) owns the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandscapeSnapshot {
    /// The repo the snapshot belongs to.
    pub repo: RepoArtifact,
    /// Every system found.
    pub systems: Vec<SystemArtifact>,
    /// Every component found, across all systems.
    pub components: Vec<ComponentArtifact>,
    /// Every advance found, across all systems.
    pub advances: Vec<AdvanceArtifact>,
}

/// What one sync run wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncSummary {
    /// Systems written.
    pub systems: usize,
    /// Components written.
    pub components: usize,
    /// Advances written.
    pub advances: usize,
}

/// One repo, as read back from the landscape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepoView {
    /// The repo id.
    pub id: String,
    /// Its display name.
    pub name: String,
    /// The `owner/name` GitHub identity ([`RepoArtifact::git_repo`]). `None`
    /// only for a row synced before ADV-GATEWAY-009 and not yet re-synced —
    /// re-running `sync` converges it, the same idempotent convergence every
    /// other landscape field already gets.
    pub git_repo: Option<String>,
}

/// One system, as read back from the landscape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemView {
    /// The system id.
    pub id: String,
    /// Its display name.
    pub name: String,
}

/// One component, as read back from the landscape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentView {
    /// The component's short id.
    pub id: String,
    /// The system it belongs to.
    pub system: String,
    /// Its display name.
    pub name: String,
    /// Its lifecycle stage, if set.
    pub stage: Option<String>,
    /// The plain names of every owner currently related to it.
    pub owners: Vec<String>,
}

/// One advance, as read back from the landscape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdvanceView {
    /// The advance id.
    pub id: String,
    /// The system it belongs to.
    pub system: String,
    /// Its title.
    pub title: String,
    /// Its frontmatter status, if set.
    pub status: Option<String>,
    /// The short component ids it currently impacts.
    pub components: Vec<String>,
}

/// The merged landscape view for one repo (docs/project-brief.md G2: "the
/// full ARRIVE landscape ... queryable in one round trip").
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct LandscapeView {
    /// The repo, if it has ever been synced.
    pub repo: Option<RepoView>,
    /// Every system belonging to it.
    pub systems: Vec<SystemView>,
    /// Every component belonging to it.
    pub components: Vec<ComponentView>,
    /// Every advance belonging to it.
    pub advances: Vec<AdvanceView>,
}

/// One completed sync run, as recorded in `sync_run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncRun {
    /// Where the ingested artifacts came from (e.g. `dogfood:058-totem-core`).
    pub source: String,
    /// When the run began.
    pub started_at: DateTime<Utc>,
    /// When the run's transaction committed.
    pub completed_at: DateTime<Utc>,
    /// Systems written by this run.
    pub systems_synced: usize,
    /// Components written by this run.
    pub components_synced: usize,
    /// Advances written by this run.
    pub advances_synced: usize,
}

fn repo_thing(id: &str) -> RecordId {
    RecordId::new(REPO_TABLE, RecordIdKey::from(id.to_string()))
}

fn system_thing(id: &str) -> RecordId {
    RecordId::new(SYSTEM_TABLE, RecordIdKey::from(id.to_string()))
}

fn component_thing(key: &str) -> RecordId {
    RecordId::new(COMPONENT_TABLE, RecordIdKey::from(key.to_string()))
}

fn advance_thing(id: &str) -> RecordId {
    RecordId::new(ADVANCE_TABLE, RecordIdKey::from(id.to_string()))
}

fn actor_thing(id: &str) -> RecordId {
    RecordId::new(ACTOR_TABLE, RecordIdKey::from(id.to_string()))
}

fn opt_string(value: Option<String>) -> Value {
    value.map_or(Value::None, |value| value.into_value())
}

/// Landscape reads and writes: the only way to mirror or query an enrolled
/// repo's ARRIVE artifacts.
#[derive(Debug)]
pub struct LandscapeRepository<'a, C: Connection> {
    db: &'a Surreal<C>,
}

impl<'a, C: Connection> LandscapeRepository<'a, C> {
    pub(crate) fn new(db: &'a Surreal<C>) -> Self {
        Self { db }
    }

    /// Ingest one snapshot.
    ///
    /// Idempotent: every entity is addressed by a deterministic id, and
    /// `UPSERT` replaces its content rather than duplicating a row. The
    /// `owned_by` edges of every synced component and the `impacts` edges of
    /// every synced advance are deleted and re-related from the snapshot, so
    /// a dropped owner or a retargeted advance does not leave a stale edge
    /// behind.
    ///
    /// Every entity, every edge, and the sync-provenance row commit in one
    /// transaction (TD-006): a partial write can never leave the landscape
    /// half-updated, and a run that fails leaves no provenance row either —
    /// `sync_run` records completed syncs, not attempts.
    pub async fn sync(
        &self,
        snapshot: &LandscapeSnapshot,
        source: &str,
    ) -> StoreResult<SyncSummary> {
        let started_at = Utc::now();
        let mut sql = String::from("BEGIN TRANSACTION;\n");
        let mut vars = Object::new();

        vars.insert("repo_id", repo_thing(&snapshot.repo.id));
        vars.insert("repo_name", snapshot.repo.name.clone());
        vars.insert("repo_git_repo", snapshot.repo.git_repo.clone());
        sql.push_str("UPSERT $repo_id CONTENT { name: $repo_name, git_repo: $repo_git_repo };\n");

        for (index, system) in snapshot.systems.iter().enumerate() {
            let id_key = format!("sys{index}_id");
            let name_key = format!("sys{index}_name");
            vars.insert(id_key.clone(), system_thing(&system.id));
            vars.insert(name_key.clone(), system.name.clone());
            sql.push_str(&format!(
                "UPSERT ${id_key} CONTENT {{ name: ${name_key}, repo: $repo_id }};\n"
            ));
        }

        for (index, component) in snapshot.components.iter().enumerate() {
            let id_key = format!("comp{index}_id");
            let short_id_key = format!("comp{index}_short_id");
            let name_key = format!("comp{index}_name");
            let stage_key = format!("comp{index}_stage");
            let system_key = format!("comp{index}_system");
            vars.insert(id_key.clone(), component_thing(&component.key()));
            vars.insert(short_id_key.clone(), component.id.clone());
            vars.insert(name_key.clone(), component.name.clone());
            vars.insert(stage_key.clone(), opt_string(component.stage.clone()));
            vars.insert(system_key.clone(), system_thing(&component.system));
            sql.push_str(&format!(
                "UPSERT ${id_key} CONTENT {{ component_id: ${short_id_key}, name: ${name_key}, \
                 stage: ${stage_key}, system: ${system_key} }};\n"
            ));
            sql.push_str(&format!("DELETE owned_by WHERE in = ${id_key};\n"));
            for (owner_index, owner) in component.owners.iter().enumerate() {
                let owner_id_key = format!("comp{index}_owner{owner_index}_id");
                let owner_name_key = format!("comp{index}_owner{owner_index}_name");
                vars.insert(owner_id_key.clone(), actor_thing(&owner.id));
                vars.insert(owner_name_key.clone(), owner.name.clone());
                sql.push_str(&format!(
                    "UPSERT ${owner_id_key} CONTENT {{ name: ${owner_name_key} }};\n"
                ));
                sql.push_str(&format!("RELATE ${id_key}->owned_by->${owner_id_key};\n"));
            }
        }

        for (index, advance) in snapshot.advances.iter().enumerate() {
            let id_key = format!("adv{index}_id");
            let title_key = format!("adv{index}_title");
            let status_key = format!("adv{index}_status");
            let system_key = format!("adv{index}_system");
            vars.insert(id_key.clone(), advance_thing(&advance.id));
            vars.insert(title_key.clone(), advance.title.clone());
            vars.insert(status_key.clone(), opt_string(advance.status.clone()));
            vars.insert(system_key.clone(), system_thing(&advance.system));
            sql.push_str(&format!(
                "UPSERT ${id_key} CONTENT {{ title: ${title_key}, status: ${status_key}, \
                 system: ${system_key} }};\n"
            ));
            sql.push_str(&format!("DELETE impacts WHERE in = ${id_key};\n"));
            for (component_index, component_id) in advance.components.iter().enumerate() {
                let target_key = format!("adv{index}_impacts{component_index}");
                let key = component_key(&advance.system, component_id);
                vars.insert(target_key.clone(), component_thing(&key));
                sql.push_str(&format!("RELATE ${id_key}->impacts->${target_key};\n"));
            }
        }

        vars.insert("source", source.to_string());
        vars.insert("started_at", row::instant(started_at));
        vars.insert("systems_synced", snapshot.systems.len() as i64);
        vars.insert("components_synced", snapshot.components.len() as i64);
        vars.insert("advances_synced", snapshot.advances.len() as i64);
        sql.push_str(
            "CREATE sync_run CONTENT { repo: $repo_id, source: $source, \
             started_at: $started_at, completed_at: time::now(), \
             systems_synced: $systems_synced, components_synced: $components_synced, \
             advances_synced: $advances_synced };\n",
        );
        sql.push_str("COMMIT TRANSACTION;");

        self.db.query(sql).bind(vars).await?.check()?;

        Ok(SyncSummary {
            systems: snapshot.systems.len(),
            components: snapshot.components.len(),
            advances: snapshot.advances.len(),
        })
    }

    /// One repo's own row, or `None` if it has never synced — the lean read
    /// a caller checking `git_repo` ownership needs (`handlers::enroll`'s
    /// rebind guard, ADV-GATEWAY-009 follow-up). Unlike [`view`](Self::view),
    /// this issues exactly one query rather than four: a caller with no use
    /// for the systems/components/advances a full view materializes
    /// shouldn't pay for them.
    pub async fn repo(&self, repo_id: &str) -> StoreResult<Option<RepoView>> {
        let repo = repo_thing(repo_id);
        let mut response = self
            .db
            .query("SELECT * FROM $repo")
            .bind(("repo", repo))
            .await?
            .check()?;

        objects(response.take(0)?)?
            .first()
            .map(|row| repo_view(repo_id, row))
            .transpose()
    }

    /// The merged landscape view for one repo, in one round trip (G2).
    pub async fn view(&self, repo_id: &str) -> StoreResult<LandscapeView> {
        let repo = repo_thing(repo_id);
        let mut response = self
            .db
            .query("SELECT * FROM $repo")
            .query("SELECT * FROM system WHERE repo = $repo")
            .query(
                "SELECT *, ->owned_by->actor.name AS owner_names FROM component \
                 WHERE system.repo = $repo",
            )
            .query(
                "SELECT *, ->impacts->component.component_id AS impacted_component_ids \
                 FROM advance WHERE system.repo = $repo",
            )
            .bind(("repo", repo))
            .await?
            .check()?;

        let repo_view = objects(response.take(0)?)?
            .first()
            .map(|row| repo_view(repo_id, row))
            .transpose()?;

        let systems = objects(response.take(1)?)?
            .iter()
            .map(|row| -> StoreResult<SystemView> {
                Ok(SystemView {
                    id: record_key(row)?,
                    name: row::string(row, "name")?,
                })
            })
            .collect::<StoreResult<Vec<_>>>()?;

        let components = objects(response.take(2)?)?
            .iter()
            .map(|row| -> StoreResult<ComponentView> {
                Ok(ComponentView {
                    id: row::string(row, "component_id")?,
                    system: linked_key(row, "system")?,
                    name: row::string(row, "name")?,
                    stage: opt_row_string(row, "stage")?,
                    owners: strings(row, "owner_names")?,
                })
            })
            .collect::<StoreResult<Vec<_>>>()?;

        let advances = objects(response.take(3)?)?
            .iter()
            .map(|row| -> StoreResult<AdvanceView> {
                Ok(AdvanceView {
                    id: record_key(row)?,
                    system: linked_key(row, "system")?,
                    title: row::string(row, "title")?,
                    status: opt_row_string(row, "status")?,
                    components: strings(row, "impacted_component_ids")?,
                })
            })
            .collect::<StoreResult<Vec<_>>>()?;

        Ok(LandscapeView {
            repo: repo_view,
            systems,
            components,
            advances,
        })
    }

    /// One advance's current status, read directly by id (ADV-GATEWAY-004
    /// gap-fill: `totem_advance_status`).
    ///
    /// Advance ids are globally unique by ARRIVE convention
    /// ([`AdvanceArtifact::id`]'s own doc), so — unlike [`view`](Self::view),
    /// which is scoped to a repo because systems and components are not
    /// addressable the same way — no repo qualifier is needed to address one.
    /// `None` means the id has never been synced, the same "not yet enrolled
    /// is a normal state, not a fault" convention `view` already establishes.
    pub async fn advance(&self, id: &str) -> StoreResult<Option<AdvanceView>> {
        let mut response = self
            .db
            .query(
                "SELECT *, ->impacts->component.component_id AS impacted_component_ids \
                 FROM $id",
            )
            .bind(("id", advance_thing(id)))
            .await?
            .check()?;

        objects(response.take(0)?)?
            .first()
            .map(|row| -> StoreResult<AdvanceView> {
                Ok(AdvanceView {
                    id: record_key(row)?,
                    system: linked_key(row, "system")?,
                    title: row::string(row, "title")?,
                    status: opt_row_string(row, "status")?,
                    components: strings(row, "impacted_component_ids")?,
                })
            })
            .transpose()
    }

    /// Every sync run recorded so far, oldest first — the provenance trail
    /// `arrive-sync.yaml` requires ("every ingestion records sync
    /// provenance").
    pub async fn sync_runs(&self) -> StoreResult<Vec<SyncRun>> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {SYNC_RUN_TABLE} ORDER BY started_at ASC"
            ))
            .await?
            .check()?;
        objects(response.take(0)?)?
            .iter()
            .map(|row| -> StoreResult<SyncRun> {
                Ok(SyncRun {
                    source: row::string(row, "source")?,
                    started_at: row::datetime(row, "started_at")?,
                    completed_at: row::datetime(row, "completed_at")?,
                    systems_synced: count(row, "systems_synced")?,
                    components_synced: count(row, "components_synced")?,
                    advances_synced: count(row, "advances_synced")?,
                })
            })
            .collect()
    }
}

fn objects(rows: Value) -> StoreResult<Vec<Object>> {
    let rows = rows
        .into_array()
        .map_err(|_| StoreError::Row("query did not return an array".to_string()))?;
    rows.iter()
        .map(|row| {
            row.clone()
                .into_object()
                .map_err(|_| StoreError::Row("query row is not an object".to_string()))
        })
        .collect()
}

/// Parses one `repo` row into a [`RepoView`] — shared by [`LandscapeRepository::repo`]
/// and [`LandscapeRepository::view`] so the two queries can't drift on what a
/// repo row means.
fn repo_view(repo_id: &str, row: &Object) -> StoreResult<RepoView> {
    Ok(RepoView {
        id: repo_id.to_string(),
        name: row::string(row, "name")?,
        git_repo: opt_row_string(row, "git_repo")?,
    })
}

/// The key half of a `record<...>`-linked field (e.g. `system` on
/// `component`/`advance`), as text. `SELECT *` returns an unfetched link as
/// a bare [`RecordId`] reference, not a nested object — this reads that
/// reference directly rather than trying to dereference it.
fn linked_key(row: &Object, key: &str) -> StoreResult<String> {
    key_of(&row::record_id(row, key)?)
}

/// The `<table>:<key>` record's key half, as text.
fn record_key(row: &Object) -> StoreResult<String> {
    key_of(&row::record_id(row, "id")?)
}

fn key_of(thing: &RecordId) -> StoreResult<String> {
    match &thing.key {
        RecordIdKey::String(key) => Ok(key.clone()),
        other => Err(malformed(format!("record key is not a string: {other:?}")).into()),
    }
}

fn opt_row_string(row: &Object, key: &str) -> StoreResult<Option<String>> {
    match row.get(key) {
        None | Some(Value::None) | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.to_string())),
        other => Err(malformed(format!("`{key}` is not a string: {other:?}")).into()),
    }
}

/// A projected array of strings, e.g. `->impacts->component.component_id`.
///
/// A traversal element is `NONE` rather than a string when the edge's target
/// record does not exist (a dangling link) — not malformed data, just an
/// absent target — so those elements are dropped rather than rejected.
fn strings(row: &Object, key: &str) -> StoreResult<Vec<String>> {
    match row.get(key) {
        None | Some(Value::None) | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .filter(|value| !matches!(value, Value::None | Value::Null))
            .map(|value| match value {
                Value::String(value) => Ok(value.to_string()),
                other => Err(malformed(format!("`{key}` holds a non-string: {other:?}")).into()),
            })
            .collect(),
        other => Err(malformed(format!("`{key}` is not an array: {other:?}")).into()),
    }
}

/// Matches `Number::Int` directly rather than routing through
/// [`row::number`]'s `f64` widening: these fields are `TYPE int` in the
/// schema, and a float round trip through `f64` could silently truncate a
/// value that should instead be reported as malformed.
fn count(row: &Object, key: &str) -> StoreResult<usize> {
    match row.get(key) {
        Some(Value::Number(Number::Int(value))) => usize::try_from(*value)
            .map_err(|_| malformed(format!("`{key}` is out of range for usize: {value}")).into()),
        other => Err(malformed(format!("`{key}` is not a non-negative integer: {other:?}")).into()),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::store::Store;

    fn unified_snapshot() -> LandscapeSnapshot {
        LandscapeSnapshot {
            repo: RepoArtifact {
                id: "058-totem".to_string(),
                name: "058 Totem".to_string(),
                git_repo: "srswart/totem".to_string(),
            },
            systems: Vec::new(),
            components: Vec::new(),
            advances: Vec::new(),
        }
    }

    /// ADV-GATEWAY-009's migration story: a `repo` row written by a
    /// pre-unification sync has no `git_repo` field at all (the shape every
    /// already-enrolled repo's row has today). Re-syncing must converge it
    /// onto the unified id — the same idempotent convergence `sync`'s own doc
    /// already promises every other landscape field — rather than requiring a
    /// bespoke migration step.
    #[tokio::test]
    async fn a_repo_synced_before_id_unification_gains_git_repo_on_the_next_sync() {
        let store = Store::in_memory().await.expect("embedded engine connects");
        store.migrate().await.expect("migrations apply");
        let db = store.connection();

        db.query("UPSERT $id CONTENT { name: $name };")
            .bind(("id", repo_thing("058-totem")))
            .bind(("name", "058 Totem"))
            .await
            .expect("pre-unification seed executes")
            .check()
            .expect("pre-unification seed has no per-statement errors");

        let landscape = LandscapeRepository::new(db);
        let pre_migration = landscape.view("058-totem").await.expect("view succeeds");
        assert_eq!(
            pre_migration.repo.expect("repo present").git_repo,
            None,
            "the seeded row has no git_repo, matching a real pre-unification row"
        );

        landscape
            .sync(&unified_snapshot(), "test")
            .await
            .expect("sync succeeds");

        let post_migration = landscape.view("058-totem").await.expect("view succeeds");
        assert_eq!(
            post_migration
                .repo
                .expect("repo present")
                .git_repo
                .as_deref(),
            Some("srswart/totem"),
        );
    }

    /// The relay's trigger (ADV-CONSOLE-003): a `watch()` subscriber must see
    /// a pulse for a committed `sync`, and must not see one before the sync
    /// happens. `verify_live_query`'s sentinel-drain pattern
    /// (`totem-store-spike`) proves the *absence* of a spurious pulse without
    /// depending on a quiet period; the timeout here proves the opposite
    /// failure mode — a subscriber that never wakes for a real committed
    /// write — deterministically instead of hanging the test suite.
    #[tokio::test]
    async fn a_committed_sync_wakes_a_watch_subscriber() {
        let store = Store::in_memory().await.expect("embedded engine connects");
        store.migrate().await.expect("migrations apply");
        let landscape = LandscapeRepository::new(store.connection());

        let mut changes = landscape.watch().await.expect("watch subscribes");

        landscape
            .sync(&unified_snapshot(), "test")
            .await
            .expect("sync commits");

        tokio::time::timeout(Duration::from_secs(5), changes.next())
            .await
            .expect("a committed sync must wake the watch subscriber before the timeout")
            .expect("the watch stream must not close on a live subscriber")
            .expect("a committed write must not surface as a stream error");
    }
}
