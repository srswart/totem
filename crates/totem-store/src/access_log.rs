//! The access log repository: one append-only row per read or write
//! (docs/project-brief.md G3; ADV-GATEWAY-001).
//!
//! Like episodic memory, an audit trail that can be rewritten is not an audit
//! trail: `access_log`'s schema (`schema.rs`) refuses `UPDATE` and `DELETE` at
//! the database level, so this repository offers no method that could attempt
//! either — there is nothing here to revise or remove.

use surrealdb::types::{Object, SurrealValue, Value};
use surrealdb::{Connection, Surreal};
use totem_core::{AccessLogEntry, AccessOperation, ActorId, MemoryId, ScopeChain, SessionId};

use crate::error::{StoreError, StoreResult};
use crate::memory::MemoryRepository;
use crate::row::{self, RowError};

const ACCESS_LOG_TABLE: &str = "access_log";

fn operation_key(operation: AccessOperation) -> &'static str {
    match operation {
        AccessOperation::Recall => "recall",
        AccessOperation::Save => "save",
        AccessOperation::Feedback => "feedback",
        AccessOperation::Propose => "propose",
        AccessOperation::PromotionDecision => "promotion_decision",
        AccessOperation::Resolve => "resolve",
    }
}

fn operation_from(key: &str) -> Result<AccessOperation, RowError> {
    match key {
        "recall" => Ok(AccessOperation::Recall),
        "save" => Ok(AccessOperation::Save),
        "feedback" => Ok(AccessOperation::Feedback),
        "propose" => Ok(AccessOperation::Propose),
        "promotion_decision" => Ok(AccessOperation::PromotionDecision),
        "resolve" => Ok(AccessOperation::Resolve),
        other => Err(row::malformed(format!(
            "unknown access log operation: {other}"
        ))),
    }
}

fn to_row(entry: &AccessLogEntry) -> Object {
    let mut row = Object::new();
    row.insert("actor", entry.actor.to_string());
    row.insert("harness", row::harness_key(&entry.harness));
    row.insert("session", entry.session.to_string());
    row.insert(
        "turn",
        entry
            .turn
            .map_or(Value::None, |turn| i64::from(turn).into_value()),
    );
    row.insert("operation", operation_key(entry.operation));
    row.insert("endpoint", entry.endpoint.clone());
    row.insert(
        "memory_id",
        entry
            .memory_id
            .map_or(Value::None, |id| row::memory_thing(id).into_value()),
    );
    row.insert(
        "result_count",
        entry
            .result_count
            .map_or(Value::None, |count| (count as i64).into_value()),
    );
    row.insert("at", row::instant(entry.at));
    row
}

fn from_row(row: &Object) -> Result<AccessLogEntry, RowError> {
    let actor = ActorId::new(row::string(row, "actor")?)
        .map_err(|error| row::malformed(format!("stored actor is invalid: {error}")))?;
    let harness = row::harness_from(&row::string(row, "harness")?)?;
    let session = SessionId::new(row::string(row, "session")?)
        .map_err(|error| row::malformed(format!("stored session is invalid: {error}")))?;
    let operation = operation_from(&row::string(row, "operation")?)?;
    let endpoint = row::string(row, "endpoint")?;
    let at = row::datetime(row, "at")?;

    let mut entry = AccessLogEntry::new(actor, harness, session, operation, endpoint, at);

    entry.turn = match row.get("turn") {
        None | Some(Value::None) | Some(Value::Null) => None,
        Some(value) => Some(
            row::number(value)
                .and_then(|turn| u32::try_from(turn as i64).ok())
                .ok_or_else(|| row::malformed("turn is not a non-negative integer"))?,
        ),
    };
    entry.memory_id = match row.get("memory_id") {
        None | Some(Value::None) | Some(Value::Null) => None,
        Some(_) => Some(row::memory_id(&row::record_id(row, "memory_id")?)?),
    };
    entry.result_count = match row.get("result_count") {
        None | Some(Value::None) | Some(Value::Null) => None,
        Some(value) => Some(
            row::number(value)
                .and_then(|count| u64::try_from(count as i64).ok())
                .ok_or_else(|| row::malformed("result_count is not a non-negative integer"))?,
        ),
    };

    Ok(entry)
}

/// Reads and writes of access log entries.
#[derive(Debug)]
pub struct AccessLogRepository<'a, C: Connection> {
    db: &'a Surreal<C>,
}

impl<'a, C: Connection> AccessLogRepository<'a, C> {
    pub(crate) fn new(db: &'a Surreal<C>) -> Self {
        Self { db }
    }

    /// Append one entry. Never fails on a duplicate — there is no identity to
    /// collide on, only a sequence of things that happened.
    pub async fn record(&self, entry: &AccessLogEntry) -> StoreResult<()> {
        self.db
            .query(format!("CREATE {ACCESS_LOG_TABLE} CONTENT $row"))
            .bind(("row", to_row(entry)))
            .await?
            .check()?;
        Ok(())
    }

    /// Every entry, oldest first — the audit query the objective promises.
    pub async fn list(&self) -> StoreResult<Vec<AccessLogEntry>> {
        let mut response = self
            .db
            .query(format!("SELECT * FROM {ACCESS_LOG_TABLE} ORDER BY at ASC"))
            .await?
            .check()?;
        rows_to_entries(response.take(0)?)
    }

    /// One memory's own access history, oldest first — the audit trail
    /// viewer's read (ADV-CONSOLE-002).
    ///
    /// Refused when the reader cannot see the memory itself
    /// ([`StoreError::NotFound`], never forbidden): an access-log entry
    /// naming an id the reader cannot see would leak that the id exists, the
    /// same concern [`crate::promotion::PromotionRepository::propose`] and
    /// [`crate::curation::CurationRepository::merge`] already re-check via
    /// [`MemoryRepository::get`] rather than trusting a caller's own prior
    /// check.
    pub async fn for_memory(
        &self,
        reader: &ScopeChain,
        id: MemoryId,
    ) -> StoreResult<Vec<AccessLogEntry>> {
        if MemoryRepository::new(self.db).get(reader, id).await?.is_none() {
            return Err(StoreError::NotFound(id));
        }

        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {ACCESS_LOG_TABLE} WHERE memory_id = $id ORDER BY at ASC"
            ))
            .bind(("id", row::memory_thing(id)))
            .await?
            .check()?;
        rows_to_entries(response.take(0)?)
    }
}

fn rows_to_entries(rows: Value) -> StoreResult<Vec<AccessLogEntry>> {
    let rows = rows
        .into_array()
        .map_err(|_| StoreError::Row("access log query did not return an array".to_string()))?;

    let mut entries = Vec::with_capacity(rows.len());
    for row in &rows {
        let row = row
            .clone()
            .into_object()
            .map_err(|_| StoreError::Row("access log row is not an object".to_string()))?;
        entries.push(from_row(&row)?);
    }
    Ok(entries)
}
