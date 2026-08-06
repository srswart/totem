//! The recall-quality scorer (ADV-GATEWAY-008): runs the golden query set
//! ADV-STORE-005's synthetic corpus ships
//! ([`totem_store::corpus::golden_queries`]) against a seeded store and
//! reports ranking metrics, so a ranking change (ADV-CORE-002) is measured
//! against a fixed baseline instead of judged by feel.

use serde::Serialize;
use surrealdb::Connection;
use totem_core::{Content, MemoryRecord, ScopeChain};
use totem_store::corpus::{self, GoldenQuery};
use totem_store::{DeterministicEmbedder, RecallQuery, Store, StoreResult, embed};

/// One golden query's outcome: what it asked for, and what recall returned.
#[derive(Debug, Clone, Serialize)]
pub struct QueryResult {
    /// The query's own label.
    pub name: &'static str,
    /// `Some(true)` if the query declared `expected_top` and it ranked
    /// first; `Some(false)` if it did not; `None` if the query does not
    /// assert a top rank.
    pub expected_top_hit: Option<bool>,
    /// How many of the query's `must_appear` bodies showed up anywhere in
    /// the result set.
    pub must_appear_hits: usize,
    /// How many `must_appear` bodies the query declared.
    pub must_appear_total: usize,
    /// How many records recall returned for this query.
    pub returned_count: usize,
}

/// The full scoring run: every golden query's outcome, plus the two
/// aggregate metrics a quality comparison reads.
#[derive(Debug, Clone, Serialize)]
pub struct QualityReport {
    /// Every golden query's individual outcome.
    pub queries: Vec<QueryResult>,
    /// Fraction of `expected_top`-bearing queries that ranked their expected
    /// record first. `None` when no golden query declares one.
    pub precision_at_1: Option<f64>,
    /// Fraction of `must_appear` bodies, summed across every query that
    /// declares any, present in that query's result set. `None` when no
    /// golden query declares any.
    pub recall_at_k: Option<f64>,
}

/// Score recall quality against the golden query set.
///
/// `reader_override`, when set, replaces every golden query's own reader
/// chain with this one — the negative control this advance's Outcome
/// requires: pass an identity with no visibility into the corpus's project
/// scope and the report's metrics collapse toward zero, proving the scorer
/// measures what its reader can actually see rather than reporting a fixed
/// number regardless of input. `None` reproduces each query's own declared
/// reader (`corpus::NOVA` throughout the current golden set) — the positive
/// control.
pub async fn score_recall_quality<C: Connection>(
    store: &Store<C>,
    reader_override: Option<&ScopeChain>,
) -> StoreResult<QualityReport> {
    let embedder = DeterministicEmbedder::new();
    let mut queries = Vec::new();

    for query in corpus::golden_queries() {
        let reader = match reader_override {
            Some(chain) => chain.clone(),
            None => {
                corpus::reader_chain(query.reader_actor, query.reader_project, query.reader_teams)
            }
        };

        let mut recall = RecallQuery::new().in_categories(query.categories.iter().copied());
        if let Some(text) = query.probe_text {
            let probe = embed(&embedder, Content::new(text))?
                .embedding
                .expect("embed always attaches a vector");
            recall = recall.near(probe)?.top_k(5);
        }
        let records = store.memories().recall(&reader, &recall).await?;

        queries.push(score_one(query, &records));
    }

    Ok(aggregate(queries))
}

fn score_one(query: GoldenQuery, records: &[MemoryRecord]) -> QueryResult {
    let expected_top_hit = query.expected_top.map(|expected| {
        records
            .first()
            .is_some_and(|record| record.content.body == expected)
    });
    let must_appear_hits = query
        .must_appear
        .iter()
        .filter(|expected| {
            records
                .iter()
                .any(|record| record.content.body == **expected)
        })
        .count();

    QueryResult {
        name: query.name,
        expected_top_hit,
        must_appear_hits,
        must_appear_total: query.must_appear.len(),
        returned_count: records.len(),
    }
}

fn aggregate(queries: Vec<QueryResult>) -> QualityReport {
    let top1: Vec<bool> = queries.iter().filter_map(|q| q.expected_top_hit).collect();
    let precision_at_1 = if top1.is_empty() {
        None
    } else {
        Some(top1.iter().filter(|hit| **hit).count() as f64 / top1.len() as f64)
    };

    let (hits, total) = queries
        .iter()
        .filter(|q| q.must_appear_total > 0)
        .fold((0usize, 0usize), |(hits, total), q| {
            (hits + q.must_appear_hits, total + q.must_appear_total)
        });
    let recall_at_k = if total == 0 {
        None
    } else {
        Some(hits as f64 / total as f64)
    };

    QualityReport {
        queries,
        precision_at_1,
        recall_at_k,
    }
}
