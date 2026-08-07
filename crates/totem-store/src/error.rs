//! What the store refuses, and why.

use totem_core::{
    CurationError, CurationId, GovernanceError, LifecycleError, MemoryId, PromotionError,
    PromotionId, Scope,
};

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
    /// No credential with this fingerprint has ever been recorded. Revoking
    /// one is an error rather than a silent no-op — see
    /// [`CredentialRepository::revoke`](crate::CredentialRepository::revoke).
    #[error("no credential is recorded with fingerprint {0}")]
    CredentialNotFound(String),
    /// The credential was revoked; re-recording the same fingerprint would
    /// undo that, so it is refused.
    #[error("credential {0} is revoked and cannot be re-recorded")]
    CredentialRevoked(String),
    /// A policy rule refused a scope change.
    #[error(transparent)]
    Promotion(#[from] PromotionError),
    /// The proposal does not exist, is already decided, or targets a scope this
    /// caller cannot reach.
    #[error("promotion {0} is not open to this caller")]
    PromotionNotFound(PromotionId),
    /// The proposal has already been approved or rejected; a second decision
    /// would put a contradiction in the audit trail.
    #[error("promotion {0} has already been decided")]
    PromotionDecided(PromotionId),
    /// A policy rule refused a curation.
    #[error(transparent)]
    Curation(#[from] CurationError),
    /// The curation event does not exist, or happened at a scope this caller
    /// cannot reach.
    #[error("curation event {0} is not visible to this caller")]
    CurationNotFound(CurationId),
    /// The merge has already been rolled back; a second rollback would claim to
    /// restore records that are already restored.
    #[error("curation event {0} has already been rolled back")]
    CurationRolledBack(CurationId),
    /// A review decision was refused by [`Governance::resolve`](totem_core::Governance::resolve)
    /// — not itself a decision, or the review is not `Pending`.
    #[error(transparent)]
    Governance(#[from] GovernanceError),
    /// The database's own guard on the resolving `UPDATE` matched no rows
    /// even though a pre-check on a fresh read passed — a review was decided
    /// between that read and this write. Same shape as
    /// [`StoreError::PromotionDecided`], and the same rare race it guards
    /// against.
    #[error("the review on memory {0} was already decided")]
    ReviewDecided(MemoryId),
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
    /// An [`Embedder`](crate::Embedder) could not produce a vector for the given text.
    #[error("embedding failed: {0}")]
    Embedding(String),
    /// SurrealDB refused the statement.
    #[error(transparent)]
    Database(#[from] surrealdb::Error),
}
