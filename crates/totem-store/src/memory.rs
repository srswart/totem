//! The memory repository: every read and write, scope-resolved.
//!
//! The type signatures are the first line of the isolation invariant. Every
//! method takes a [`ScopeChain`], and a chain can only be built by
//! [`ScopeChain::resolve`], which names one actor's own private scope and
//! nobody else's. There is no method that reads without one, and no way to
//! reach the connection to write one by hand.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use surrealdb::types::{Object, SurrealValue, Value};
use surrealdb::{Connection, Surreal};
use totem_core::{Content, MemoryCategory, MemoryId, MemoryRecord, Scope, ScopeChain, SubjectKind};

use crate::error::{StoreError, StoreResult};
use crate::row::{self, MEMORY_TABLE};
use crate::schema::EMBEDDING_DIMENSIONS;

/// How many rows a recall returns when the caller does not say.
const DEFAULT_LIMIT: usize = 20;
/// The HNSW `efSearch` default.
///
/// TD-003 is explicit that five rows cannot establish a floor for this: treat
/// it as a tuning parameter with an unmeasured minimum until ADV-STORE-005
/// measures recall against a realistic corpus.
const DEFAULT_SEARCH_EFFORT: usize = 40;

/// What to recall, and how to rank it.
///
/// The temporal cutoff is a [`DateTime<Utc>`] and nothing else. TD-004 records
/// what a string-bound instant does in SurrealQL — compares by type rank,
/// filters nothing, raises no error — so the type here is what makes that
/// mistake unexpressible rather than merely discouraged.
#[derive(Debug, Clone, PartialEq)]
pub struct RecallQuery {
    probe: Option<Vec<f32>>,
    top_k: usize,
    search_effort: usize,
    categories: Vec<MemoryCategory>,
    since: Option<DateTime<Utc>>,
    limit: usize,
}

impl Default for RecallQuery {
    fn default() -> Self {
        Self::new()
    }
}

impl RecallQuery {
    /// Everything the reader may see, newest first.
    pub fn new() -> Self {
        Self {
            probe: None,
            top_k: DEFAULT_LIMIT,
            search_effort: DEFAULT_SEARCH_EFFORT,
            categories: Vec::new(),
            since: None,
            limit: DEFAULT_LIMIT,
        }
    }

    /// Rank by vector proximity to `embedding`.
    ///
    /// Refuses anything but a [`EMBEDDING_DIMENSIONS`]-dimension vector: the
    /// index is pinned to that width, and a mismatched probe is a caller bug
    /// worth naming rather than a database error worth forwarding.
    pub fn near(mut self, embedding: Vec<f32>) -> StoreResult<Self> {
        check_dimensions(&embedding)?;
        self.probe = Some(embedding);
        Ok(self)
    }

    /// How many rows the vector search returns (the HNSW `K`).
    pub fn top_k(mut self, k: usize) -> Self {
        self.top_k = k;
        self
    }

    /// The HNSW `efSearch` parameter.
    pub fn search_effort(mut self, ef: usize) -> Self {
        self.search_effort = ef;
        self
    }

    /// Restrict to these categories. Empty means every category.
    pub fn in_categories(mut self, categories: impl IntoIterator<Item = MemoryCategory>) -> Self {
        self.categories = categories.into_iter().collect();
        self
    }

    /// Only records written strictly after `cutoff`.
    pub fn since(mut self, cutoff: DateTime<Utc>) -> Self {
        self.since = Some(cutoff);
        self
    }

    /// Cap the merged result set.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// The statement, with the scope predicate the store generates.
    ///
    /// `K` and `ef` are formatted rather than bound because the knn operator
    /// takes them as literals, not parameters. They are `usize` and never
    /// caller-supplied text, so nothing here is a string the caller controls —
    /// every value that *is* caller-supplied (scopes, categories, cutoff,
    /// probe) is a bound parameter.
    fn statement(&self, scope_count: usize) -> String {
        let mut sql = String::from("SELECT *");
        if self.probe.is_some() {
            sql.push_str(", vector::distance::knn() AS knn_distance");
        }
        sql.push_str(" FROM ");
        sql.push_str(MEMORY_TABLE);
        sql.push_str(" WHERE ");

        if self.probe.is_some() {
            sql.push_str(&format!(
                "embedding <|{},{}|> $probe AND ",
                self.top_k, self.search_effort
            ));
        }
        sql.push_str("scope IN $scopes");
        if !self.categories.is_empty() {
            sql.push_str(" AND category IN $categories");
        }
        if self.since.is_some() {
            sql.push_str(" AND provenance.created_at > $since");
        }

        if self.probe.is_some() {
            // The knn operator has already bounded the row count at K.
            sql.push_str(" ORDER BY knn_distance ASC");
        } else {
            // Over-fetch by the width of the chain: the merge below collapses
            // at most one duplicate per scope, so this is the smallest fetch
            // that cannot return fewer than `limit` rows for want of candidates.
            sql.push_str(&format!(
                " ORDER BY provenance.created_at DESC LIMIT {}",
                self.limit.saturating_mul(scope_count.max(1))
            ));
        }
        sql
    }
}

fn check_dimensions(embedding: &[f32]) -> StoreResult<()> {
    if embedding.len() == EMBEDDING_DIMENSIONS {
        return Ok(());
    }
    Err(StoreError::EmbeddingDimensions {
        expected: EMBEDDING_DIMENSIONS,
        actual: embedding.len(),
    })
}

/// Reads and writes of memory records.
#[derive(Debug)]
pub struct MemoryRepository<'a, C: Connection> {
    db: &'a Surreal<C>,
}

impl<'a, C: Connection> MemoryRepository<'a, C> {
    pub(crate) fn new(db: &'a Surreal<C>) -> Self {
        Self { db }
    }

    /// Persist a new record.
    ///
    /// Refused when the record's scope is not one the writer's chain contains:
    /// a caller cannot deposit memory into another actor's private scope, nor
    /// into a team it does not belong to.
    pub async fn save(&self, writer: &ScopeChain, record: &MemoryRecord) -> StoreResult<()> {
        if !writer.contains(&record.scope) {
            return Err(StoreError::ScopeDenied {
                scope: record.scope.clone(),
            });
        }
        if let Some(embedding) = &record.content.embedding {
            check_dimensions(embedding)?;
        }

        self.db
            .query(format!("INSERT INTO {MEMORY_TABLE} $row"))
            .bind(("row", row::to_row(record)))
            .await?
            .check()?;
        Ok(())
    }

    /// Read one record, if the reader's chain permits it.
    ///
    /// A record outside the chain is reported as absent rather than forbidden.
    /// The distinction would otherwise let a caller confirm that another
    /// actor's memory exists, which is a leak even when the body never travels.
    pub async fn get(
        &self,
        reader: &ScopeChain,
        id: MemoryId,
    ) -> StoreResult<Option<MemoryRecord>> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {MEMORY_TABLE} WHERE id = $id AND scope IN $scopes"
            ))
            .bind(("id", row::memory_thing(id)))
            .bind(("scopes", readable_scopes(reader)))
            .await?
            .check()?;

        let rows = objects(response.take(0)?)?;
        rows.first()
            .map(|row| row::from_row(row).map_err(StoreError::from))
            .transpose()
    }

    /// Replace a record's content.
    ///
    /// Refused for append-only categories, and refused for records the writer
    /// cannot see. Scope is untouched: sharing is by promotion, which is a
    /// recorded event (ADV-CORE-003), never a side effect of an edit.
    pub async fn revise(
        &self,
        writer: &ScopeChain,
        id: MemoryId,
        content: Content,
    ) -> StoreResult<MemoryRecord> {
        let Some(mut record) = self.get(writer, id).await? else {
            return Err(StoreError::NotFound(id));
        };
        if let Some(embedding) = &content.embedding {
            check_dimensions(embedding)?;
        }
        // The category rule lives in the domain model; asking it here means
        // there is one answer to "may this be rewritten", not two.
        record.revise(content)?;

        self.db
            .query(format!(
                "UPDATE {MEMORY_TABLE} SET body = $body, embedding = $embedding, tags = $tags
                 WHERE id = $id AND scope IN $scopes"
            ))
            .bind(("id", row::memory_thing(id)))
            .bind(("scopes", readable_scopes(writer)))
            .bind(("body", record.content.body.clone()))
            .bind((
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
            ))
            .bind(("tags", record.content.tags.clone()))
            .await?
            .check()?;
        Ok(record)
    }

    /// The merged, deduplicated view across the reader's whole chain.
    pub async fn recall(
        &self,
        reader: &ScopeChain,
        query: &RecallQuery,
    ) -> StoreResult<Vec<MemoryRecord>> {
        let rows = objects(self.run_recall(reader, query, false).await?)?;
        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            records.push(row::from_row(row)?);
        }
        Ok(merge_chain(reader, records, query.limit))
    }

    /// The `EXPLAIN FULL` plan for the statement [`recall`](Self::recall) would
    /// run.
    ///
    /// Public because it is the only way to prove *where* the scope filter ran.
    /// TD-002 is explicit that results alone cannot distinguish an indexed knn
    /// from a full scan, and TD-003 that the scope predicate belongs inside the
    /// index scan — so the plan is a standing regression guard, not a debugging
    /// convenience.
    pub async fn explain_recall(
        &self,
        reader: &ScopeChain,
        query: &RecallQuery,
    ) -> StoreResult<String> {
        let plan: Value = self.run_recall(reader, query, true).await?;
        Ok(format!("{plan:?}"))
    }

    async fn run_recall(
        &self,
        reader: &ScopeChain,
        query: &RecallQuery,
        explain: bool,
    ) -> StoreResult<Value> {
        let mut sql = query.statement(reader.scopes().len());
        if explain {
            sql.push_str(" EXPLAIN FULL");
        }

        let mut request = self
            .db
            .query(sql)
            .bind(("scopes", readable_scopes(reader)))
            .bind((
                "categories",
                query
                    .categories
                    .iter()
                    .map(|category| row::category_key(*category).to_string())
                    .collect::<Vec<String>>(),
            ));
        if let Some(probe) = &query.probe {
            request = request.bind((
                "probe",
                probe.iter().copied().map(f64::from).collect::<Vec<f64>>(),
            ));
        }
        if let Some(since) = query.since {
            request = request.bind(("since", row::instant(since)));
        }

        let mut response = request.await?.check()?;
        Ok(response.take(0)?)
    }
}

/// The scopes a reader may see, as the store's own predicate values.
///
/// Derived from the chain, never from a caller-supplied filter: the widest set
/// a caller can ask for is the set it already had.
fn readable_scopes(reader: &ScopeChain) -> Vec<String> {
    reader
        .scopes()
        .iter()
        .map(Scope::to_string)
        .collect::<Vec<String>>()
}

fn objects(rows: Value) -> StoreResult<Vec<Object>> {
    let rows = rows
        .into_array()
        .map_err(|_| StoreError::Row("recall did not return an array".to_string()))?;
    rows.iter()
        .map(|row| {
            row.clone()
                .into_object()
                .map_err(|_| StoreError::Row("recall row is not an object".to_string()))
        })
        .collect()
}

/// The key two records must share to be the same fact held at two scopes.
///
/// Body comparison is whitespace- and case-insensitive so that a personal copy
/// of a project rule collapses into it. Anything looser would merge records
/// that merely resemble each other; anything stricter would leave a caller
/// reading the same instruction twice.
type DedupKey = (MemoryCategory, Option<(SubjectKind, String)>, String);

fn dedup_key(record: &MemoryRecord) -> DedupKey {
    let subject = record
        .subject
        .as_ref()
        .map(|subject| (subject.kind, subject.id.clone()));
    let body = record
        .content
        .body
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
        .to_lowercase();
    (record.category, subject, body)
}

/// Resolve the chain into one view: narrowest scope wins, duplicates collapse,
/// and statement order (rank, or recency) is preserved among the survivors.
fn merge_chain(reader: &ScopeChain, records: Vec<MemoryRecord>, limit: usize) -> Vec<MemoryRecord> {
    let mut winners: HashMap<DedupKey, usize> = HashMap::new();

    for (position, record) in records.iter().enumerate() {
        // Defence in depth. The statement already filtered on the chain, so
        // this should never drop anything — but the cost of being wrong here is
        // another actor's private memory, and the plan assertion in
        // tests/scope_isolation.rs is what proves the predicate itself is still
        // in the query rather than only in this loop.
        let Some(precedence) = reader.precedence_of(&record.scope) else {
            continue;
        };

        winners
            .entry(dedup_key(record))
            .and_modify(|held| {
                let incumbent = &records[*held];
                let incumbent_precedence =
                    reader.precedence_of(&incumbent.scope).unwrap_or(usize::MAX);
                let closer = precedence < incumbent_precedence;
                let newer = precedence == incumbent_precedence
                    && record.provenance.created_at > incumbent.provenance.created_at;
                if closer || newer {
                    *held = position;
                }
            })
            .or_insert(position);
    }

    let mut kept: Vec<usize> = winners.into_values().collect();
    kept.sort_unstable();
    kept.into_iter()
        .take(limit)
        .map(|position| records[position].clone())
        .collect()
}
