//! Schema migrations: an ordered, recorded, replay-safe list.

use chrono::{DateTime, Utc};

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
pub const MIGRATIONS: &[Migration] = &[];

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
