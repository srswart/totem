//! Schema migrations: an ordered, recorded, replay-safe list.
//!
//! Forward-only and never edited once merged. The ledger records what a given
//! database has run, so `migrate()` is safe on every start-up and a database
//! that has already caught up executes no DDL at all.

use chrono::{DateTime, Utc};

use crate::schema::MEMORY_SCHEMA_V1;

/// One forward-only schema change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    /// Its position in the ordered sequence.
    pub version: u32,
    /// A human-readable name, recorded in the ledger.
    pub name: &'static str,
    /// The SurrealQL applied when this migration runs.
    pub statements: &'static str,
}

/// Every migration, in the order they must be applied.
///
/// Append only: editing a migration that has already run somewhere leaves that
/// database on a schema no entry describes.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "typed_memory_and_landscape",
    statements: MEMORY_SCHEMA_V1,
}];

/// A migration this database has already run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMigration {
    /// Which migration.
    pub version: u32,
    /// Its name at the time it was applied.
    pub name: String,
    /// When it was applied.
    pub applied_at: DateTime<Utc>,
}
