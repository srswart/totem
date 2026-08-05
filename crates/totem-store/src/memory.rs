//! The memory repository: every read and write, scope-resolved.

use chrono::{DateTime, Utc};
use surrealdb::Connection;
use totem_core::{Content, MemoryCategory, MemoryId, MemoryRecord, ScopeChain};

use crate::error::StoreResult;

/// What to recall, and how to rank it.
#[derive(Debug, Clone, Default)]
pub struct RecallQuery {
    #[allow(dead_code)]
    probe: Option<Vec<f32>>,
}

impl RecallQuery {
    /// Everything the reader may see, newest first.
    pub fn new() -> Self {
        unimplemented!("ADV-STORE-001")
    }

    /// Rank by vector proximity to `embedding`.
    pub fn near(self, embedding: Vec<f32>) -> StoreResult<Self> {
        let _ = embedding;
        unimplemented!("ADV-STORE-001")
    }

    /// How many rows the vector search returns.
    pub fn top_k(self, k: usize) -> Self {
        let _ = k;
        unimplemented!("ADV-STORE-001")
    }

    /// The HNSW `efSearch` parameter.
    pub fn search_effort(self, ef: usize) -> Self {
        let _ = ef;
        unimplemented!("ADV-STORE-001")
    }

    /// Restrict to these categories.
    pub fn in_categories(self, categories: impl IntoIterator<Item = MemoryCategory>) -> Self {
        let _ = categories.into_iter().count();
        unimplemented!("ADV-STORE-001")
    }

    /// Only records written strictly after `cutoff`.
    pub fn since(self, cutoff: DateTime<Utc>) -> Self {
        let _ = cutoff;
        unimplemented!("ADV-STORE-001")
    }

    /// Cap the merged result set.
    pub fn limit(self, limit: usize) -> Self {
        let _ = limit;
        unimplemented!("ADV-STORE-001")
    }
}

/// Reads and writes of memory records.
#[derive(Debug)]
pub struct MemoryRepository<'a, C: Connection> {
    #[allow(dead_code)]
    db: &'a surrealdb::Surreal<C>,
}

impl<C: Connection> MemoryRepository<'_, C> {
    /// Persist a new record.
    pub async fn save(&self, writer: &ScopeChain, record: &MemoryRecord) -> StoreResult<()> {
        let _ = (writer, record);
        unimplemented!("ADV-STORE-001")
    }

    /// Read one record, if the reader's chain permits it.
    pub async fn get(
        &self,
        reader: &ScopeChain,
        id: MemoryId,
    ) -> StoreResult<Option<MemoryRecord>> {
        let _ = (reader, id);
        unimplemented!("ADV-STORE-001")
    }

    /// Replace a record's content.
    pub async fn revise(
        &self,
        writer: &ScopeChain,
        id: MemoryId,
        content: Content,
    ) -> StoreResult<MemoryRecord> {
        let _ = (writer, id, content);
        unimplemented!("ADV-STORE-001")
    }

    /// The merged, deduplicated view across the reader's whole chain.
    pub async fn recall(
        &self,
        reader: &ScopeChain,
        query: &RecallQuery,
    ) -> StoreResult<Vec<MemoryRecord>> {
        let _ = (reader, query);
        unimplemented!("ADV-STORE-001")
    }

    /// The `EXPLAIN FULL` plan for the statement [`recall`](Self::recall) would run.
    pub async fn explain_recall(
        &self,
        reader: &ScopeChain,
        query: &RecallQuery,
    ) -> StoreResult<String> {
        let _ = (reader, query);
        unimplemented!("ADV-STORE-001")
    }
}
