//! The connection, its migrations, and the repositories reached through it.

use surrealdb::Connection;
use surrealdb::engine::local::Db;

use crate::error::StoreResult;
use crate::memory::MemoryRepository;
use crate::migrate::AppliedMigration;

/// A connected Totem store.
#[derive(Debug, Clone)]
pub struct Store<C: Connection> {
    db: surrealdb::Surreal<C>,
}

impl Store<Db> {
    /// Connect a fresh embedded in-memory instance.
    pub async fn in_memory() -> StoreResult<Self> {
        unimplemented!("ADV-STORE-001")
    }
}

impl<C: Connection> Store<C> {
    /// Apply every migration this database has not run yet, returning the
    /// versions applied by this call.
    pub async fn migrate(&self) -> StoreResult<Vec<u32>> {
        unimplemented!("ADV-STORE-001")
    }

    /// The migrations this database has already run, oldest first.
    pub async fn applied_migrations(&self) -> StoreResult<Vec<AppliedMigration>> {
        unimplemented!("ADV-STORE-001")
    }

    /// The memory repository.
    pub fn memories(&self) -> MemoryRepository<'_, C> {
        unimplemented!("ADV-STORE-001")
    }

    /// The raw connection.
    ///
    /// Deliberately crate-private. A public accessor would let a caller write
    /// a statement with no scope predicate, which is exactly the failure the
    /// repository API exists to make unexpressible.
    pub(crate) fn connection(&self) -> &surrealdb::Surreal<C> {
        &self.db
    }
}
