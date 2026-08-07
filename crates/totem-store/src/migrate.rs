//! Schema migrations: an ordered, recorded, replay-safe list.
//!
//! Forward-only and never edited once merged. The ledger records what a given
//! database has run, so `migrate()` is safe on every start-up and a database
//! that has already caught up executes no DDL at all.

use chrono::{DateTime, Utc};

use crate::schema::{
    ACCESS_LOG_FEEDBACK_SCHEMA_V4, ACCESS_LOG_GOVERNANCE_SCHEMA_V7, ACCESS_LOG_REFUSAL_SCHEMA_V9,
    ACCESS_LOG_SCHEMA_V2, CURATION_SCHEMA_V6, LANDSCAPE_SYNC_SCHEMA_V3, MEMORY_SCHEMA_V1,
    PROMOTION_SCHEMA_V5, REPO_GIT_IDENTITY_SCHEMA_V8,
};

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
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "typed_memory_and_landscape",
        statements: MEMORY_SCHEMA_V1,
    },
    Migration {
        version: 2,
        name: "access_log",
        statements: ACCESS_LOG_SCHEMA_V2,
    },
    Migration {
        version: 3,
        name: "landscape_sync",
        statements: LANDSCAPE_SYNC_SCHEMA_V3,
    },
    Migration {
        version: 4,
        name: "access_log_feedback_operation",
        statements: ACCESS_LOG_FEEDBACK_SCHEMA_V4,
    },
    Migration {
        version: 5,
        name: "scope_promotion_events",
        statements: PROMOTION_SCHEMA_V5,
    },
    Migration {
        version: 6,
        name: "curation_events",
        statements: CURATION_SCHEMA_V6,
    },
    Migration {
        version: 7,
        name: "access_log_governance_operations",
        statements: ACCESS_LOG_GOVERNANCE_SCHEMA_V7,
    },
    Migration {
        version: 8,
        name: "repo_git_identity",
        statements: REPO_GIT_IDENTITY_SCHEMA_V8,
    },
    Migration {
        version: 9,
        name: "access_log_refusals",
        statements: ACCESS_LOG_REFUSAL_SCHEMA_V9,
    },
];

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
