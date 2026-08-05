//! What the store refuses, and why.

use totem_core::{LifecycleError, MemoryId, Scope};

/// The result of a store operation.
pub type StoreResult<T> = Result<T, StoreError>;

/// Why a store operation did not happen.
///
/// Note what is deliberately *absent*: there is no "found it, but you may not
/// read it" variant. A record outside the caller's chain reads as absent
/// ([`MemoryRepository::get`](crate::MemoryRepository::get) returns `None`), so
/// an error message can never confirm that another actor's memory exists.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// A write named a scope the writer's chain does not contain.
    #[error("the caller's scope chain does not permit writing to {scope}")]
    ScopeDenied {
        /// The scope the write named.
        scope: Scope,
    },
    /// A category rule refused the operation.
    #[error(transparent)]
    Lifecycle(#[from] LifecycleError),
    /// The record does not exist, or is not visible to this caller.
    #[error("memory {0} is not present in the caller's scope chain")]
    NotFound(MemoryId),
    /// An embedding did not match the dimension the vector index is pinned to.
    #[error("an embedding must have exactly {expected} dimensions, but this one has {actual}")]
    EmbeddingDimensions {
        /// The pinned dimension count ([`EMBEDDING_DIMENSIONS`](crate::EMBEDDING_DIMENSIONS)).
        expected: usize,
        /// What the caller supplied.
        actual: usize,
    },
    /// A stored row could not be read back as a domain record.
    #[error("stored row is not a readable memory record: {0}")]
    Row(String),
    /// SurrealDB refused the statement.
    #[error(transparent)]
    Database(#[from] surrealdb::Error),
}
