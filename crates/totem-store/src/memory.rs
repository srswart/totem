//! The memory repository: every read and write, scope-resolved.
//!
//! The type signatures are the first line of the isolation invariant. Every
//! method takes a [`ScopeChain`], and a chain can only be built by
//! [`ScopeChain::resolve`], which names one actor's own private scope and
//! nobody else's. There is no method that reads without one, and no way to
//! reach the connection to write one by hand.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use surrealdb::types::{SurrealValue, Value};
use surrealdb::{Connection, Surreal};
use totem_core::{
    Content, FeedbackSignal, LifecycleError, MemoryCategory, MemoryId, MemoryRecord, ReviewState,
    ScopeChain, SubjectKind,
};

use crate::embedding::Embedder;
use crate::error::{StoreError, StoreResult};
use crate::row::{self, MEMORY_TABLE, objects, readable_scopes};
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
        // Retired records are withdrawn from retrieval and kept for audit
        // (`MemoryStatus::Retired`). Without this predicate a curator's
        // supersession would be a label on a row that still competes for the
        // agent's context window, and dedupe would never actually dedupe.
        // Contested records stay: a contradiction the reader should see is not
        // the same as a fact that has been withdrawn.
        sql.push_str(" AND governance.status != $retired");
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
/// What a re-embed pass did, for the operator and the advance record.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReembedSummary {
    /// Rows with an embedding, considered by the pass.
    pub examined: usize,
    /// Rows rewritten into the target model's space.
    pub reembedded: usize,
    /// Rows already in that space, left untouched.
    pub skipped: usize,
}

#[derive(Debug, surrealdb::types::SurrealValue)]
struct ReembedRow {
    id: surrealdb::types::RecordId,
    body: String,
    embedding_model: Option<String>,
}

#[derive(Debug, surrealdb::types::SurrealValue)]
struct EmbeddingModelCount {
    embedding_model: Option<String>,
    rows: usize,
}

pub(crate) fn check_dimensions(embedding: &[f32]) -> StoreResult<()> {
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
    ///
    /// If the record names sources it was derived from
    /// (`provenance.derived_from`), each cited source's `value_score` is
    /// raised (VAL-002, docs/tech-direction/value-attribution.md): citation
    /// is the one signal this investigation found real discriminating power
    /// in. The boost only ever reaches a source the *writer's own chain* can
    /// see — a citation naming an id outside it is silently a no-op, the same
    /// as any other write this repository refuses to let cross a scope
    /// boundary. The insert and the citation boost commit as one transaction
    /// (TD-006) so a failure in the citation update can never leave the new
    /// record inserted without it — `save` stays all-or-nothing rather than
    /// risking a duplicate insert on a caller's retry.
    pub async fn save(&self, writer: &ScopeChain, record: &MemoryRecord) -> StoreResult<()> {
        if !writer.contains(&record.scope) {
            return Err(StoreError::ScopeDenied {
                scope: record.scope.clone(),
            });
        }
        if let Some(embedding) = &record.content.embedding {
            check_dimensions(embedding)?;
        }

        if record.provenance.derived_from.is_empty() {
            self.db
                .query(format!("INSERT INTO {MEMORY_TABLE} $row"))
                .bind(("row", row::to_row(record)))
                .await?
                .check()?;
            return Ok(());
        }

        let ids: Vec<Value> = record
            .provenance
            .derived_from
            .iter()
            .copied()
            .map(|id| row::memory_thing(id).into_value())
            .collect();
        let sql = format!(
            "BEGIN TRANSACTION;\n\
             INSERT INTO {MEMORY_TABLE} $row;\n\
             UPDATE {MEMORY_TABLE} SET economics.value_score += $boost \
                 WHERE id IN $ids AND scope IN $scopes AND category != $episodic;\n\
             COMMIT TRANSACTION;"
        );
        self.db
            .query(sql)
            .bind(("row", row::to_row(record)))
            .bind(("ids", ids))
            .bind(("boost", CITATION_BOOST))
            .bind(("scopes", readable_scopes(writer)))
            .bind(("episodic", row::category_key(MemoryCategory::Episodic)))
            .await?
            .check()?;
        Ok(())
    }

    /// How many rows each embedding model wrote, most rows first.
    ///
    /// The operator's answer to "is this index in one space?". A single entry
    /// means yes; more than one means recall is ranking across geometries and
    /// its ordering is not meaningful. An entry keyed on the empty string is
    /// the pre-ADV-STORE-008 rows that carry no label at all.
    pub async fn embedding_models(&self) -> StoreResult<Vec<(String, usize)>> {
        let mut response = self
            .db
            .query(format!(
                "SELECT embedding_model, count() AS rows FROM {MEMORY_TABLE} \
                 WHERE embedding != NONE GROUP BY embedding_model"
            ))
            .await?
            .check()?;
        let rows: Vec<EmbeddingModelCount> = response.take(0)?;
        let mut counts: Vec<(String, usize)> = rows
            .into_iter()
            .map(|row| (row.embedding_model.unwrap_or_default(), row.rows))
            .collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Ok(counts)
    }

    /// Re-embed every row not already in `embedder`'s space.
    ///
    /// **Why this is not a start-up migration.** DEP-001 makes the gateway the
    /// store's sole owner, so this must run inside that process — but running
    /// it at boot would hold the health check open for the whole pass on a
    /// deployment whose machine count is one, and would re-run on every
    /// restart with no operator deciding it should. It is an explicit call
    /// instead, so it can follow the backup step the advance's risk section
    /// requires.
    ///
    /// **Why it is all-or-nothing on error.** A pass that skipped rows it
    /// could not embed would return success over an index that is still
    /// mixed, which is the precise condition this exists to eliminate. A
    /// failure leaves already-rewritten rows in place — those are correct, and
    /// re-running resumes from where it stopped, which is what the label
    /// buys.
    pub async fn reembed_all(&self, embedder: &dyn Embedder) -> StoreResult<ReembedSummary> {
        let target = embedder.model_name();
        let mut response = self
            .db
            .query(format!(
                "SELECT id, body, embedding_model FROM {MEMORY_TABLE} WHERE embedding != NONE"
            ))
            .await?
            .check()?;
        let rows: Vec<ReembedRow> = response.take(0)?;

        let mut summary = ReembedSummary {
            examined: rows.len(),
            ..ReembedSummary::default()
        };
        for row in rows {
            if row.embedding_model.as_deref() == Some(target) {
                summary.skipped += 1;
                continue;
            }
            let vector = embedder.embed(&row.body)?;
            check_dimensions(&vector)?;
            self.db
                .query("UPDATE $id SET embedding = $embedding, embedding_model = $model")
                .bind(("id", row.id.clone()))
                .bind((
                    "embedding",
                    vector.into_iter().map(f64::from).collect::<Vec<f64>>(),
                ))
                .bind(("model", target.to_string()))
                .await?
                .check()?;
            summary.reembedded += 1;
        }
        Ok(summary)
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

    /// Apply an explicit feedback signal to a record's economics
    /// (docs/solution-intent.md §4; ADV-GATEWAY-004 gap-fill — the explicit
    /// input side of the value loop the automatic citation boost (`save`) and
    /// usage reinforcement (`recall`) feed alongside).
    ///
    /// Refused for append-only categories and for records the writer cannot
    /// see, the same two refusals [`revise`](Self::revise) makes and for the
    /// same reasons: the schema itself refuses `UPDATE` on an episodic row
    /// (schema.rs), and a record outside the chain must read as absent, not
    /// forbidden.
    pub async fn apply_feedback(
        &self,
        writer: &ScopeChain,
        id: MemoryId,
        signal: FeedbackSignal,
    ) -> StoreResult<MemoryRecord> {
        let Some(mut record) = self.get(writer, id).await? else {
            return Err(StoreError::NotFound(id));
        };
        if record.category.is_append_only() {
            return Err(LifecycleError::AppendOnly(record.category).into());
        }

        record.economics = totem_core::apply_feedback(&record.economics, signal);

        self.db
            .query(format!(
                "UPDATE {MEMORY_TABLE} SET economics.value_score = $value_score, \
                 economics.currency = $currency WHERE id = $id AND scope IN $scopes"
            ))
            .bind(("id", row::memory_thing(id)))
            .bind(("scopes", readable_scopes(writer)))
            .bind(("value_score", f64::from(record.economics.value_score)))
            .bind(("currency", f64::from(record.economics.currency)))
            .await?
            .check()?;
        Ok(record)
    }

    /// Records of `category` awaiting a human decision, oldest first — the
    /// queue ADV-CONSOLE-002 renders (its own use is
    /// [`MemoryCategory::Uncertainty`]); scope-filtered to the reader's chain
    /// the same way every other read here is.
    pub async fn pending_review(
        &self,
        reader: &ScopeChain,
        category: MemoryCategory,
    ) -> StoreResult<Vec<MemoryRecord>> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {MEMORY_TABLE} WHERE category = $category \
                 AND governance.review = $pending AND scope IN $scopes \
                 ORDER BY provenance.created_at ASC"
            ))
            .bind(("category", row::category_key(category).to_string()))
            .bind(("pending", row::review_key(ReviewState::Pending).to_string()))
            .bind(("scopes", readable_scopes(reader)))
            .await?
            .check()?;
        objects(response.take(0)?)?
            .iter()
            .map(|row| row::from_row(row).map_err(StoreError::from))
            .collect()
    }

    /// Record a human's decision on a pending review — approve or reject
    /// (ADV-CONSOLE-002: the Uncertainty queue's resolution step, and equally
    /// usable on any other human-gated category's record).
    ///
    /// Refused for a record the resolver cannot see ([`StoreError::NotFound`],
    /// never forbidden, same as [`revise`](Self::revise)), and refused unless
    /// the record's review is currently `Pending`
    /// ([`totem_core::GovernanceError`] via [`totem_core::Governance::resolve`]).
    /// The resolving `UPDATE` repeats that `governance.review = 'pending'`
    /// guard at the database, so a decision that raced this one between the
    /// read above and this write is refused too
    /// ([`StoreError::ReviewDecided`]) rather than silently overwritten —
    /// the same defence-in-depth [`totem_core::PromotionEvent`]'s decision
    /// guard applies, and the same unserialised-race residual it discloses.
    pub async fn resolve_review(
        &self,
        resolver: &ScopeChain,
        id: MemoryId,
        decision: ReviewState,
    ) -> StoreResult<MemoryRecord> {
        let Some(mut record) = self.get(resolver, id).await? else {
            return Err(StoreError::NotFound(id));
        };
        record.governance = record.governance.resolve(decision)?;

        let mut response = self
            .db
            .query(format!(
                "UPDATE {MEMORY_TABLE} SET governance.review = $decision \
                 WHERE id = $id AND scope IN $scopes AND governance.review = $pending"
            ))
            .bind(("id", row::memory_thing(id)))
            .bind(("scopes", readable_scopes(resolver)))
            .bind(("decision", row::review_key(decision).to_string()))
            .bind(("pending", row::review_key(ReviewState::Pending).to_string()))
            .await?
            .check()?;
        let moved = objects(response.take(0)?)?;
        if moved.len() != 1 {
            return Err(StoreError::ReviewDecided(id));
        }
        Ok(record)
    }

    /// The merged, deduplicated view across the reader's whole chain, ranked
    /// by combined score — relevance × value × currency, weighted per
    /// category (docs/solution-intent.md §4) — and not merely by the
    /// statement's own vector-rank or recency order.
    ///
    /// Every record actually returned counts as a use: its `use_count`
    /// increments, `last_used_at` moves to now, and `currency` refreshes to
    /// full trust (reinforcement). Episodic records are exempt — the schema
    /// refuses `UPDATE` on them outright (schema.rs), so touching one here
    /// would fail the whole recall, not just skip it; `is_append_only`
    /// excludes them before any statement is built.
    pub async fn recall(
        &self,
        reader: &ScopeChain,
        query: &RecallQuery,
    ) -> StoreResult<Vec<MemoryRecord>> {
        let rows = objects(self.run_recall(reader, query, false).await?)?;
        let mut scored = Vec::with_capacity(rows.len());
        for row in &rows {
            // Only a vector query projects this column; `row::from_row`
            // ignores it as an "extra projected column" the domain model has
            // no field for (its own doc comment) — carried alongside instead
            // of dropped, since ranking needs it (ADV-CORE-002).
            let distance = row.get("knn_distance").and_then(row::number);
            scored.push((row::from_row(row)?, distance));
        }
        let merged = merge_chain(reader, scored);

        let now = Utc::now();
        let mut ranked: Vec<(MemoryRecord, f32)> = merged
            .into_iter()
            .map(|(record, distance)| {
                let score = rank_score(&record, distance, now);
                (record, score)
            })
            .collect();
        // Stable: records tied on score keep the order the statement (and
        // merge_chain's precedence rule) already gave them.
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        ranked.truncate(query.limit);

        let records: Vec<MemoryRecord> =
            ranked.into_iter().map(|(record, _score)| record).collect();
        self.reinforce_usage(reader, &records, now).await?;
        Ok(records)
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
                "retired",
                row::status_key(totem_core::MemoryStatus::Retired).to_string(),
            ))
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

    /// Meter a recall: `use_count` up by one, `last_used_at` to `at`,
    /// `currency` refreshed to full trust, for every returned record the
    /// category rules allow touching at all. Episodic records are filtered
    /// out before the statement is even built (`is_append_only`), and the
    /// `category != $episodic` predicate is the same defence-in-depth the
    /// rest of this module already applies to scope filtering: two
    /// independent reasons the append-only invariant cannot be crossed here,
    /// not one.
    async fn reinforce_usage(
        &self,
        reader: &ScopeChain,
        records: &[MemoryRecord],
        at: DateTime<Utc>,
    ) -> StoreResult<()> {
        let ids: Vec<Value> = records
            .iter()
            .filter(|record| !record.category.is_append_only())
            .map(|record| row::memory_thing(record.id).into_value())
            .collect();
        if ids.is_empty() {
            return Ok(());
        }

        self.db
            .query(format!(
                "UPDATE {MEMORY_TABLE}
                 SET economics.use_count += 1, economics.last_used_at = $at, economics.currency = 1.0
                 WHERE id IN $ids AND scope IN $scopes AND category != $episodic"
            ))
            .bind(("ids", ids))
            .bind(("at", row::instant(at)))
            .bind(("scopes", readable_scopes(reader)))
            .bind(("episodic", row::category_key(MemoryCategory::Episodic)))
            .await?
            .check()?;
        Ok(())
    }
}

/// How much a citation (a new record's `provenance.derived_from` link)
/// raises the cited record's `value_score`. VAL-002
/// (docs/tech-direction/value-attribution.md) scored citation at precision
/// 0.80 on the labeled corpus — the only signal with real, if noisy,
/// discriminating power available today (VAL-003 rules out weighting raw
/// retrieval directly; VAL-005 has zero explicit-feedback data points). One
/// flat constant, not per-category tuning: the investigation's per-category
/// weights (§5) govern `category_weight` in ranking, not this boost.
const CITATION_BOOST: f64 = 0.2;

/// Combined ranking score for one candidate: relevance × value × currency,
/// weighted by category (docs/solution-intent.md §4). `currency` is computed
/// live from the record's own stored value and how long it has sat since it
/// was last used (or, if never used, since it was written) — nothing here
/// persists a decayed value; only [`MemoryRepository::reinforce_usage`]
/// writes to `currency`, and only ever resets it to full trust.
fn rank_score(record: &MemoryRecord, distance: Option<f64>, now: DateTime<Utc>) -> f32 {
    let reference = record
        .economics
        .last_used_at
        .unwrap_or(record.provenance.created_at);
    let elapsed = now - reference;
    let currency =
        totem_core::effective_currency(record.category, record.economics.currency, elapsed);
    let relevance = totem_core::relevance_from_distance(distance);
    let weight = totem_core::category_weight(record.category);
    totem_core::combined_score(relevance, record.economics.value_score, currency, weight)
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

/// A candidate record paired with the vector distance it was ranked by, when
/// the query carried a probe at all.
type Scored = (MemoryRecord, Option<f64>);

/// Resolve the chain into one view: narrowest scope wins, duplicates collapse,
/// and statement order (rank, or recency) is preserved among the survivors.
///
/// Deliberately does not truncate to a limit itself: the caller ranks the
/// survivors by combined score first (ADV-CORE-002) and truncates after, so
/// capping here could discard a record the final ranking would have kept.
fn merge_chain(reader: &ScopeChain, records: Vec<Scored>) -> Vec<Scored> {
    let mut winners: HashMap<DedupKey, usize> = HashMap::new();

    for (position, (record, _distance)) in records.iter().enumerate() {
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
                let incumbent = &records[*held].0;
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
        .map(|position| records[position].clone())
        .collect()
}
