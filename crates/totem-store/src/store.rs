//! The connection, its migrations, and the repositories reached through it.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use surrealdb::engine::local::{Db, Mem};
use surrealdb::types::{Number, RecordId, Value};
use surrealdb::{Connection, Surreal};

use crate::access_log::AccessLogRepository;
use crate::credential::CredentialRepository;
use crate::curation::CurationRepository;
use crate::error::{StoreError, StoreResult};
use crate::landscape::LandscapeRepository;
use crate::memory::MemoryRepository;
use crate::migrate::{AppliedMigration, MIGRATIONS};
use crate::promotion::PromotionRepository;
use crate::row;
use crate::schema::MIGRATION_LEDGER;

/// The namespace and database every Totem instance uses.
const NAMESPACE: &str = "totem";
const DATABASE: &str = "totem";

/// A connected Totem store.
///
/// Per TD-011 the connection is a fully-privileged system user: a restricted
/// SurrealDB role is not a row filter — it still reads every scope, and its
/// writes are discarded *silently*, which no error check can detect. All
/// authorization therefore lives in this crate, and the connection is not
/// reachable from outside it.
#[derive(Debug, Clone)]
pub struct Store<C: Connection> {
    db: Surreal<C>,
}

impl Store<Db> {
    /// Connect a fresh embedded in-memory instance.
    ///
    /// This is the engine every test uses (docs/tech-direction/surrealdb.md
    /// §4) and the one the console's live queries need (TD-009). Each call
    /// yields an independent instance, so tests never share state.
    pub async fn in_memory() -> StoreResult<Self> {
        let db = Surreal::new::<Mem>(()).await?;
        db.use_ns(NAMESPACE).use_db(DATABASE).await?;
        Ok(Self { db })
    }

    /// Connect the embedded on-disk RocksDB engine at `data_dir` (DEP-001).
    ///
    /// The engine takes an exclusive lock on the directory: a second process
    /// (or a second `Store`) opening the same path fails, which is what makes
    /// the gateway the sole owner of the store physically rather than by
    /// convention. Data survives drop/reopen; callers still run [`migrate`]
    /// on every start-up.
    ///
    /// [`migrate`]: Store::migrate
    #[cfg(feature = "rocksdb")]
    pub async fn on_disk(data_dir: &std::path::Path) -> StoreResult<Self> {
        let db = Surreal::new::<surrealdb::engine::local::RocksDb>(data_dir).await?;
        db.use_ns(NAMESPACE).use_db(DATABASE).await?;
        Ok(Self { db })
    }
}

impl<C: Connection> Store<C> {
    /// Adopt an already-connected SurrealDB instance whose namespace and
    /// database are selected.
    pub fn from_connection(db: Surreal<C>) -> Self {
        Self { db }
    }

    /// Apply every migration this database has not run yet, returning the
    /// versions applied by this call.
    ///
    /// Safe to call on every start-up: a database already at the latest version
    /// gets an empty result and runs no schema statements.
    pub async fn migrate(&self) -> StoreResult<Vec<u32>> {
        self.db.query(MIGRATION_LEDGER).await?.check()?;

        let already_applied: HashSet<u32> = self
            .applied_migrations()
            .await?
            .into_iter()
            .map(|migration| migration.version)
            .collect();

        let mut applied = Vec::new();
        for migration in MIGRATIONS {
            if already_applied.contains(&migration.version) {
                continue;
            }
            self.db.query(migration.statements).await?.check()?;
            self.db
                .query("CREATE $id CONTENT { version: $version, name: $name, applied_at: $at }")
                .bind((
                    "id",
                    RecordId::new("schema_migration", i64::from(migration.version)),
                ))
                .bind(("version", i64::from(migration.version)))
                .bind(("name", migration.name.to_string()))
                .bind(("at", row::instant(Utc::now())))
                .await?
                .check()?;
            applied.push(migration.version);
        }
        Ok(applied)
    }

    /// The migrations this database has already run, oldest first.
    pub async fn applied_migrations(&self) -> StoreResult<Vec<AppliedMigration>> {
        let mut response = self
            .db
            .query("SELECT version, name, applied_at FROM schema_migration ORDER BY version ASC")
            .await?
            .check()?;
        let rows: Value = response.take(0)?;
        let rows = rows
            .into_array()
            .map_err(|_| StoreError::Row("migration ledger is not an array".to_string()))?;

        let mut applied = Vec::with_capacity(rows.len());
        for row in rows.iter() {
            let row = row.clone().into_object().map_err(|_| {
                StoreError::Row("migration ledger row is not an object".to_string())
            })?;

            let version = match row.get("version") {
                Some(Value::Number(Number::Int(version))) => {
                    u32::try_from(*version).map_err(|_| {
                        StoreError::Row(format!("migration version out of range: {version}"))
                    })?
                }
                other => {
                    return Err(StoreError::Row(format!(
                        "migration version is not an integer: {other:?}"
                    )));
                }
            };
            let name = match row.get("name") {
                Some(Value::String(name)) => name.to_string(),
                other => {
                    return Err(StoreError::Row(format!(
                        "migration name is not a string: {other:?}"
                    )));
                }
            };
            let applied_at = match row.get("applied_at") {
                Some(Value::Datetime(at)) => DateTime::<Utc>::from(*at),
                other => {
                    return Err(StoreError::Row(format!(
                        "migration timestamp is not a datetime: {other:?}"
                    )));
                }
            };

            applied.push(AppliedMigration {
                version,
                name,
                applied_at,
            });
        }
        Ok(applied)
    }

    /// The memory repository — the only way to read or write memory.
    pub fn memories(&self) -> MemoryRepository<'_, C> {
        MemoryRepository::new(&self.db)
    }

    /// The access log — the audit trail of every read and write
    /// (docs/project-brief.md G3).
    pub fn access_log(&self) -> AccessLogRepository<'_, C> {
        AccessLogRepository::new(&self.db)
    }

    /// Scope promotions under the standing policy — the only way a record's
    /// scope ever changes (docs/solution-intent.md §2.2).
    pub fn promotions(&self) -> PromotionRepository<'_, C> {
        self.promotions_with_policy(totem_core::PromotionPolicy::new())
    }

    /// Scope promotions under a policy of the caller's choosing.
    ///
    /// The configuration lever ADV-CORE-003's rollback plan names: pass
    /// [`PromotionPolicy::human_gated_everywhere`] to put every category behind
    /// a human without editing a category definition.
    ///
    /// [`PromotionPolicy::human_gated_everywhere`]: totem_core::PromotionPolicy::human_gated_everywhere
    pub fn promotions_with_policy(
        &self,
        policy: totem_core::PromotionPolicy,
    ) -> PromotionRepository<'_, C> {
        PromotionRepository::new(&self.db, policy)
    }

    /// Curator merges and their rollbacks under the standing policy — the only
    /// way a record is ever retired (`components/curator.yaml`: curation never
    /// deletes).
    /// Durable credential grants (ADV-GATEWAY-012).
    pub fn credentials(&self) -> CredentialRepository<'_, C> {
        CredentialRepository { db: &self.db }
    }

    pub fn curation(&self) -> CurationRepository<'_, C> {
        self.curation_with_policy(totem_core::CurationPolicy::new())
    }

    /// Curation under a policy of the caller's choosing.
    pub fn curation_with_policy(
        &self,
        policy: totem_core::CurationPolicy,
    ) -> CurationRepository<'_, C> {
        CurationRepository::new(&self.db, policy)
    }

    /// The landscape mirror — the only way to ingest or query an enrolled
    /// repo's ARRIVE artifacts (docs/solution-intent.md §2.3).
    pub fn landscape(&self) -> LandscapeRepository<'_, C> {
        LandscapeRepository::new(&self.db)
    }

    /// The raw connection.
    ///
    /// Deliberately crate-private. A public accessor would let a caller write a
    /// statement with no scope predicate, which is exactly the failure the
    /// repository API exists to make unexpressible.
    #[cfg(test)]
    pub(crate) fn connection(&self) -> &Surreal<C> {
        &self.db
    }
}
