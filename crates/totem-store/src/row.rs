//! Mapping between the domain record and its stored row.
//!
//! Deliberately explicit rather than derived from `serde`. Two reasons:
//!
//! - **Types, not text.** `chrono`'s serde representation of a timestamp is an
//!   RFC 3339 *string*, and TD-004 records what a string-typed instant does in
//!   SurrealQL: `created_at > $since` compares by type rank, filters nothing,
//!   and raises no error. Every instant crossing this boundary becomes a
//!   `surrealdb::types::Datetime`, in one place, where that can be checked.
//! - **A stable stored shape.** Renaming a domain variant should not silently
//!   change what is already on disk. The category, harness, status, and review
//!   spellings below are the persistence contract; changing one is a migration.

use chrono::{DateTime, Utc};
use surrealdb::types::{Datetime, Number, Object, RecordId, RecordIdKey, SurrealValue, Value};
use totem_core::{
    ActorId, Author, Content, Economics, Governance, Harness, MemoryCategory, MemoryId,
    MemoryRecord, MemoryStatus, Provenance, ReviewState, Scope, ScopeChain, SessionId, SubjectKind,
    SubjectRef,
};

/// The table memory records live in.
pub(crate) const MEMORY_TABLE: &str = "memory";

/// A stored row that could not be read back as a domain record.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub(crate) struct RowError(String);

impl From<RowError> for crate::StoreError {
    fn from(error: RowError) -> Self {
        crate::StoreError::Row(error.0)
    }
}

pub(crate) fn malformed(what: impl std::fmt::Display) -> RowError {
    RowError(what.to_string())
}

/// `memory:<uuid>` for a domain id.
pub(crate) fn memory_thing(id: MemoryId) -> RecordId {
    RecordId::new(MEMORY_TABLE, RecordIdKey::from(id.to_string()))
}

pub(crate) fn memory_id(thing: &RecordId) -> Result<MemoryId, RowError> {
    let RecordIdKey::String(key) = &thing.key else {
        return Err(malformed(format!(
            "memory id is not a string key: {thing:?}"
        )));
    };
    key.parse()
        .map_err(|_| malformed(format!("memory id is not a uuid: {key}")))
}

fn subject_table(kind: SubjectKind) -> &'static str {
    match kind {
        SubjectKind::Repo => "repo",
        SubjectKind::System => "system",
        SubjectKind::Component => "component",
        SubjectKind::Advance => "advance",
        SubjectKind::Actor => "actor",
        SubjectKind::Memory => MEMORY_TABLE,
    }
}

fn subject_kind(table: &str) -> Result<SubjectKind, RowError> {
    match table {
        "repo" => Ok(SubjectKind::Repo),
        "system" => Ok(SubjectKind::System),
        "component" => Ok(SubjectKind::Component),
        "advance" => Ok(SubjectKind::Advance),
        "actor" => Ok(SubjectKind::Actor),
        MEMORY_TABLE => Ok(SubjectKind::Memory),
        other => Err(malformed(format!("unknown subject table: {other}"))),
    }
}

pub(crate) fn category_key(category: MemoryCategory) -> &'static str {
    match category {
        MemoryCategory::Episodic => "episodic",
        MemoryCategory::Identity => "identity",
        MemoryCategory::Knowledge => "knowledge",
        MemoryCategory::Context => "context",
        MemoryCategory::Instructions => "instructions",
        MemoryCategory::Uncertainty => "uncertainty",
    }
}

fn category_from(key: &str) -> Result<MemoryCategory, RowError> {
    MemoryCategory::ALL
        .into_iter()
        .find(|category| category_key(*category) == key)
        .ok_or_else(|| malformed(format!("unknown memory category: {key}")))
}

fn author_kind_key(author: &Author) -> &'static str {
    match author {
        Author::Human(_) => "human",
        Author::Agent(_) => "agent",
        Author::Curator(_) => "curator",
    }
}

fn author_from(kind: &str, actor: ActorId) -> Result<Author, RowError> {
    match kind {
        "human" => Ok(Author::Human(actor)),
        "agent" => Ok(Author::Agent(actor)),
        "curator" => Ok(Author::Curator(actor)),
        other => Err(malformed(format!("unknown author kind: {other}"))),
    }
}

/// `Harness::Other` keeps its payload behind an `other:` prefix so the stored
/// value stays a single string, and so a harness Totem learns the name of later
/// can be promoted to a named variant by a migration.
pub(crate) fn harness_key(harness: &Harness) -> String {
    match harness {
        Harness::ClaudeCode => "claude_code".to_string(),
        Harness::Cursor => "cursor".to_string(),
        Harness::CloudAgent => "cloud_agent".to_string(),
        Harness::Console => "console".to_string(),
        Harness::Curator => "curator".to_string(),
        Harness::Other(name) => format!("other:{name}"),
    }
}

pub(crate) fn harness_from(key: &str) -> Result<Harness, RowError> {
    match key {
        "claude_code" => Ok(Harness::ClaudeCode),
        "cursor" => Ok(Harness::Cursor),
        "cloud_agent" => Ok(Harness::CloudAgent),
        "console" => Ok(Harness::Console),
        "curator" => Ok(Harness::Curator),
        other => other
            .strip_prefix("other:")
            .map(|name| Harness::Other(name.to_string()))
            .ok_or_else(|| malformed(format!("unknown harness: {other}"))),
    }
}

fn status_key(status: MemoryStatus) -> &'static str {
    match status {
        MemoryStatus::Active => "active",
        MemoryStatus::Contested => "contested",
        MemoryStatus::Retired => "retired",
    }
}

fn status_from(key: &str) -> Result<MemoryStatus, RowError> {
    match key {
        "active" => Ok(MemoryStatus::Active),
        "contested" => Ok(MemoryStatus::Contested),
        "retired" => Ok(MemoryStatus::Retired),
        other => Err(malformed(format!("unknown memory status: {other}"))),
    }
}

fn review_key(review: ReviewState) -> &'static str {
    match review {
        ReviewState::NotRequired => "not_required",
        ReviewState::Pending => "pending",
        ReviewState::Approved => "approved",
        ReviewState::Rejected => "rejected",
    }
}

fn review_from(key: &str) -> Result<ReviewState, RowError> {
    match key {
        "not_required" => Ok(ReviewState::NotRequired),
        "pending" => Ok(ReviewState::Pending),
        "approved" => Ok(ReviewState::Approved),
        "rejected" => Ok(ReviewState::Rejected),
        other => Err(malformed(format!("unknown review state: {other}"))),
    }
}

/// The one place a `DateTime<Utc>` becomes a SurrealQL instant (TD-004).
pub(crate) fn instant(value: DateTime<Utc>) -> Datetime {
    Datetime::from(value)
}

/// The scopes a reader may see, as the store's own predicate values.
///
/// Derived from the chain, never from a caller-supplied filter: the widest set
/// a caller can ask for is the set it already had. Every table that carries a
/// scope binds its predicate from here, so there is one answer to "what may
/// this caller see" rather than one per repository.
pub(crate) fn readable_scopes(reader: &ScopeChain) -> Vec<String> {
    reader
        .scopes()
        .iter()
        .map(Scope::to_string)
        .collect::<Vec<String>>()
}

/// Unpack a query result into its rows.
pub(crate) fn objects(rows: Value) -> Result<Vec<Object>, RowError> {
    let rows = rows
        .into_array()
        .map_err(|_| malformed("query did not return an array"))?;
    rows.iter()
        .map(|row| {
            row.clone()
                .into_object()
                .map_err(|_| malformed("query row is not an object"))
        })
        .collect()
}

/// The stored shape of provenance, shared by every table that records it.
///
/// Extracted so a second recorded event cannot grow a second, subtly different
/// spelling of the same audit fields: the persistence contract for "who wrote
/// this, from where, when" is written once.
pub(crate) fn provenance_to_row(value: &Provenance) -> Object {
    let mut provenance = Object::new();
    provenance.insert("author_kind", author_kind_key(&value.author));
    provenance.insert("author", value.author.actor().to_string());
    provenance.insert("harness", harness_key(&value.harness));
    provenance.insert("session", value.session.to_string());
    provenance.insert(
        "turn",
        value
            .turn
            .map_or(Value::None, |turn| i64::from(turn).into_value()),
    );
    provenance.insert("created_at", instant(value.created_at));
    provenance.insert(
        "derived_from",
        value
            .derived_from
            .iter()
            .map(|source| memory_thing(*source))
            .collect::<Vec<_>>(),
    );
    provenance
}

/// Read stored provenance back into the domain type.
pub(crate) fn provenance_from_row(provenance_row: &Object) -> Result<Provenance, RowError> {
    let author = author_from(
        &string(provenance_row, "author_kind")?,
        ActorId::new(string(provenance_row, "author")?)
            .map_err(|error| malformed(format!("stored author is invalid: {error}")))?,
    )?;
    let mut provenance = Provenance::new(
        author,
        harness_from(&string(provenance_row, "harness")?)?,
        SessionId::new(string(provenance_row, "session")?)
            .map_err(|error| malformed(format!("stored session is invalid: {error}")))?,
        datetime(provenance_row, "created_at")?,
    );
    provenance.turn = match provenance_row.get("turn") {
        None | Some(Value::None) | Some(Value::Null) => None,
        Some(value) => Some(
            number(value)
                .and_then(|turn| u32::try_from(turn as i64).ok())
                .ok_or_else(|| malformed("turn is not a non-negative integer"))?,
        ),
    };
    provenance.derived_from = match provenance_row.get("derived_from") {
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| match value {
                Value::RecordId(thing) => memory_id(thing),
                other => Err(malformed(format!(
                    "derived_from holds a non-record: {other:?}"
                ))),
            })
            .collect::<Result<Vec<MemoryId>, RowError>>()?,
        _ => Vec::new(),
    };
    Ok(provenance)
}

/// The stored shape of one memory record, ready to `INSERT`.
pub(crate) fn to_row(record: &MemoryRecord) -> Object {
    let provenance = provenance_to_row(&record.provenance);

    let mut economics = Object::new();
    economics.insert("use_count", record.economics.use_count as i64);
    economics.insert(
        "last_used_at",
        record
            .economics
            .last_used_at
            .map_or(Value::None, |at| instant(at).into_value()),
    );
    economics.insert("value_score", f64::from(record.economics.value_score));
    economics.insert("currency", f64::from(record.economics.currency));

    let mut governance = Object::new();
    governance.insert("status", status_key(record.governance.status));
    governance.insert("review", review_key(record.governance.review));

    let mut row = Object::new();
    row.insert("id", memory_thing(record.id));
    row.insert("category", category_key(record.category));
    row.insert("scope", record.scope.to_string());
    row.insert(
        "subject",
        record.subject.as_ref().map_or(Value::None, |subject| {
            RecordId::new(
                subject_table(subject.kind),
                RecordIdKey::from(subject.id.clone()),
            )
            .into_value()
        }),
    );
    row.insert("body", record.content.body.clone());
    row.insert(
        "embedding",
        record
            .content
            .embedding
            .clone()
            .map_or(Value::None, |embedding| {
                embedding
                    .into_iter()
                    .map(f64::from)
                    .collect::<Vec<f64>>()
                    .into_value()
            }),
    );
    row.insert("tags", record.content.tags.clone());
    row.insert("provenance", provenance);
    row.insert("economics", economics);
    row.insert("governance", governance);
    row
}

/// Read a stored row back as a domain record, ignoring any extra projected
/// column (a ranked recall carries a distance the domain model has no field
/// for).
pub(crate) fn from_row(row: &Object) -> Result<MemoryRecord, RowError> {
    let id = memory_id(&record_id(row, "id")?)?;
    let category = category_from(&string(row, "category")?)?;
    let scope: Scope = string(row, "scope")?
        .parse()
        .map_err(|error| malformed(format!("stored scope is not a scope: {error}")))?;

    let subject = match row.get("subject") {
        None | Some(Value::None) | Some(Value::Null) => None,
        Some(_) => {
            let thing = record_id(row, "subject")?;
            let kind = subject_kind(thing.table.as_str())?;
            let RecordIdKey::String(key) = &thing.key else {
                return Err(malformed(format!("subject key is not a string: {thing:?}")));
            };
            Some(
                SubjectRef::new(kind, key.clone())
                    .map_err(|error| malformed(format!("stored subject is invalid: {error}")))?,
            )
        }
    };

    let mut content = Content::new(string(row, "body")?);
    content.embedding = match row.get("embedding") {
        None | Some(Value::None) | Some(Value::Null) => None,
        Some(Value::Array(values)) => Some(
            values
                .iter()
                .map(|value| {
                    number(value)
                        .map(|component| component as f32)
                        .ok_or_else(|| malformed("embedding component is not a number"))
                })
                .collect::<Result<Vec<f32>, RowError>>()?,
        ),
        Some(other) => return Err(malformed(format!("embedding is not an array: {other:?}"))),
    };
    content.tags = strings(row, "tags")?;

    let provenance = provenance_from_row(&object(row, "provenance")?)?;

    let economics_row = object(row, "economics")?;
    let economics = Economics {
        use_count: number(economics_row.get("use_count").unwrap_or(&Value::None))
            .and_then(|count| u64::try_from(count as i64).ok())
            .ok_or_else(|| malformed("use_count is not a non-negative integer"))?,
        last_used_at: match economics_row.get("last_used_at") {
            None | Some(Value::None) | Some(Value::Null) => None,
            Some(_) => Some(datetime(&economics_row, "last_used_at")?),
        },
        value_score: number(economics_row.get("value_score").unwrap_or(&Value::None))
            .ok_or_else(|| malformed("value_score is not a number"))? as f32,
        currency: number(economics_row.get("currency").unwrap_or(&Value::None))
            .ok_or_else(|| malformed("currency is not a number"))? as f32,
    };

    let governance_row = object(row, "governance")?;
    let governance = Governance {
        status: status_from(&string(&governance_row, "status")?)?,
        review: review_from(&string(&governance_row, "review")?)?,
    };

    Ok(MemoryRecord {
        id,
        category,
        scope,
        subject,
        content,
        provenance,
        economics,
        governance,
    })
}

pub(crate) fn field<'a>(row: &'a Object, key: &str) -> Result<&'a Value, RowError> {
    row.get(key)
        .ok_or_else(|| malformed(format!("stored row has no `{key}`")))
}

pub(crate) fn string(row: &Object, key: &str) -> Result<String, RowError> {
    match field(row, key)? {
        Value::String(value) => Ok(value.to_string()),
        other => Err(malformed(format!("`{key}` is not a string: {other:?}"))),
    }
}

fn strings(row: &Object, key: &str) -> Result<Vec<String>, RowError> {
    match row.get(key) {
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| match value {
                Value::String(value) => Ok(value.to_string()),
                other => Err(malformed(format!("`{key}` holds a non-string: {other:?}"))),
            })
            .collect(),
        None | Some(Value::None) | Some(Value::Null) => Ok(Vec::new()),
        Some(other) => Err(malformed(format!("`{key}` is not an array: {other:?}"))),
    }
}

fn object(row: &Object, key: &str) -> Result<Object, RowError> {
    match field(row, key)? {
        Value::Object(value) => Ok(value.clone()),
        other => Err(malformed(format!("`{key}` is not an object: {other:?}"))),
    }
}

pub(crate) fn record_id(row: &Object, key: &str) -> Result<RecordId, RowError> {
    match field(row, key)? {
        Value::RecordId(thing) => Ok(thing.clone()),
        other => Err(malformed(format!("`{key}` is not a record id: {other:?}"))),
    }
}

pub(crate) fn datetime(row: &Object, key: &str) -> Result<DateTime<Utc>, RowError> {
    match field(row, key)? {
        Value::Datetime(value) => Ok(DateTime::<Utc>::from(*value)),
        // A stored instant that arrives as a string is TD-004 in the flesh:
        // report it rather than coercing, because the comparison it was
        // supposed to take part in already silently did nothing.
        other => Err(malformed(format!("`{key}` is not a datetime: {other:?}"))),
    }
}

pub(crate) fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(Number::Int(value)) => Some(*value as f64),
        Value::Number(Number::Float(value)) => Some(*value),
        _ => None,
    }
}
