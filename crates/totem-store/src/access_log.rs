//! The access log repository: one append-only row per read or write
//! (docs/project-brief.md G3; ADV-GATEWAY-001).
//!
//! Like episodic memory, an audit trail that can be rewritten is not an audit
//! trail: `access_log`'s schema (`schema.rs`) refuses `UPDATE` and `DELETE` at
//! the database level, so this repository offers no method that could attempt
//! either — there is nothing here to revise or remove.

use surrealdb::types::{Object, SurrealValue, Value};
use surrealdb::{Connection, Surreal};
use totem_core::{
    AccessLogEntry, AccessOperation, ActorId, MemoryId, RefusalReason, ScopeChain, SessionId,
};

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
        AccessOperation::Refused => "refused",
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
        "refused" => Ok(AccessOperation::Refused),
        other => Err(row::malformed(format!(
            "unknown access log operation: {other}"
        ))),
    }
}

fn refusal_reason_key(reason: RefusalReason) -> &'static str {
    match reason {
        RefusalReason::MissingCredential => "missing_credential",
        RefusalReason::UnknownCredential => "unknown_credential",
        RefusalReason::Expired => "expired",
        RefusalReason::ActorNotBound => "actor_not_bound",
        RefusalReason::RepoNotBound => "repo_not_bound",
        RefusalReason::ScopeNotBound => "scope_not_bound",
    }
}

fn refusal_reason_from(key: &str) -> Result<RefusalReason, RowError> {
    match key {
        "missing_credential" => Ok(RefusalReason::MissingCredential),
        "unknown_credential" => Ok(RefusalReason::UnknownCredential),
        "expired" => Ok(RefusalReason::Expired),
        "actor_not_bound" => Ok(RefusalReason::ActorNotBound),
        "repo_not_bound" => Ok(RefusalReason::RepoNotBound),
        "scope_not_bound" => Ok(RefusalReason::ScopeNotBound),
        other => Err(row::malformed(format!("unknown refusal reason: {other}"))),
    }
}

fn to_row(entry: &AccessLogEntry) -> Object {
    let mut row = Object::new();
    row.insert(
        "actor",
        entry
            .actor
            .as_ref()
            .map_or(Value::None, |actor| actor.to_string().into_value()),
    );
    row.insert(
        "harness",
        entry.harness.as_ref().map_or(Value::None, |harness| {
            row::harness_key(harness).into_value()
        }),
    );
    row.insert(
        "session",
        entry
            .session
            .as_ref()
            .map_or(Value::None, |session| session.to_string().into_value()),
    );
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
    row.insert(
        "refusal_reason",
        entry.refusal_reason.map_or(Value::None, |reason| {
            refusal_reason_key(reason).into_value()
        }),
    );
    row.insert(
        "credential_fingerprint",
        entry
            .credential_fingerprint
            .clone()
            .map_or(Value::None, |fingerprint| fingerprint.into_value()),
    );
    row.insert("at", row::instant(entry.at));
    row
}

fn optional_string(row: &Object, field: &str) -> Result<Option<String>, RowError> {
    match row.get(field) {
        None | Some(Value::None) | Some(Value::Null) => Ok(None),
        Some(_) => Ok(Some(row::string(row, field)?)),
    }
}

fn from_row(row: &Object) -> Result<AccessLogEntry, RowError> {
    let actor = optional_string(row, "actor")?
        .map(ActorId::new)
        .transpose()
        .map_err(|error| row::malformed(format!("stored actor is invalid: {error}")))?;
    let harness = optional_string(row, "harness")?
        .map(|harness| row::harness_from(&harness))
        .transpose()?;
    let session = optional_string(row, "session")?
        .map(SessionId::new)
        .transpose()
        .map_err(|error| row::malformed(format!("stored session is invalid: {error}")))?;
    let operation = operation_from(&row::string(row, "operation")?)?;
    // `totem-core`'s own contract (`access_log.rs`'s doc comment): actor,
    // harness, and session are `None` only on a `Refused` entry — every
    // other operation confirmed an identity before it could touch the
    // store. Enforced here, not just at the write path (`to_row` never
    // constructs a row that would violate it, but a hand-written or
    // pre-migration row could), so a row that breaks the contract is a
    // decode-time `RowError`, not a silent `Option` a reader has to
    // remember to check.
    if operation != AccessOperation::Refused
        && (actor.is_none() || harness.is_none() || session.is_none())
    {
        return Err(row::malformed(format!(
            "a {operation:?} access log row is missing actor/harness/session — only a refused entry may omit them"
        )));
    }
    let endpoint = row::string(row, "endpoint")?;
    let at = row::datetime(row, "at")?;

    let mut entry = AccessLogEntry {
        actor,
        harness,
        session,
        turn: None,
        operation,
        endpoint,
        memory_id: None,
        result_count: None,
        refusal_reason: None,
        credential_fingerprint: None,
        at,
    };

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
    entry.refusal_reason = optional_string(row, "refusal_reason")?
        .map(|reason| refusal_reason_from(&reason))
        .transpose()?;
    entry.credential_fingerprint = optional_string(row, "credential_fingerprint")?;

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
        if MemoryRepository::new(self.db)
            .get(reader, id)
            .await?
            .is_none()
        {
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

#[cfg(test)]
mod tests {
    //! `from_row`/`to_row` are crate-private, so the row-shape enforcement
    //! this module owns can only be exercised by writing a row directly
    //! (bypassing `to_row`, which never produces a row that violates the
    //! contract) — the same reason `schema.rs`'s own DB-level tests live
    //! inside the crate rather than in `tests/`.

    use crate::Store;

    async fn migrated() -> Store<surrealdb::engine::local::Db> {
        let store = Store::in_memory().await.expect("embedded engine connects");
        store.migrate().await.expect("migrations apply");
        store
    }

    #[tokio::test]
    async fn a_non_refused_row_missing_its_identity_is_a_malformed_row_not_a_silent_option() {
        let store = migrated().await;

        // A hand-written row lacking `actor`/`harness`/`session` — exactly
        // the shape a corrupted or pre-contract row could take. `to_row`
        // never produces this for a `Save` entry; this proves `from_row`
        // still catches it if something else does.
        store
            .connection()
            .query(
                "CREATE access_log CONTENT {
                    operation: 'save', endpoint: '/save',
                    at: d'2026-08-05T06:00:00Z'
                }",
            )
            .await
            .expect("sent")
            .check()
            .expect("the schema itself accepts the row — actor/harness/session are optional columns now");

        let refused = store.access_log().list().await;
        assert!(
            matches!(refused, Err(crate::StoreError::Row(_))),
            "expected a malformed-row refusal, got {refused:?}",
        );
    }

    #[tokio::test]
    async fn a_refused_row_with_no_identity_decodes_cleanly() {
        let store = migrated().await;

        store
            .connection()
            .query(
                "CREATE access_log CONTENT {
                    operation: 'refused', endpoint: '/save',
                    refusal_reason: 'missing_credential',
                    at: d'2026-08-05T06:00:00Z'
                }",
            )
            .await
            .expect("sent")
            .check()
            .expect("a refused row with no identity satisfies the schema");

        let entries = store.access_log().list().await.expect("decodes cleanly");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].actor, None);
    }
}
