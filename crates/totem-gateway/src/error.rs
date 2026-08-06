//! How the gateway turns a refusal into an HTTP response.
//!
//! Deliberately narrow: the only variant is a wrapped [`StoreError`], because
//! every rule this API enforces — scope isolation, append-only categories,
//! embedding dimensions — already lives in `totem-store`; the gateway does not
//! duplicate it. Malformed JSON (an invalid [`totem_core::Scope`], an empty
//! actor id) never reaches here at all — Axum's `Json` extractor rejects it
//! before a handler runs, since every request field is a `totem-core` type
//! with its own validating `Deserialize`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use totem_store::StoreError;

/// Why a request was refused, after it parsed successfully.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// The store refused the operation.
    #[error(transparent)]
    Store(#[from] StoreError),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let status = match &self {
            // A write into a scope the writer's own chain does not contain —
            // unlike a read miss, this reveals nothing about another actor's
            // memory, so it is an ordinary permissions refusal.
            GatewayError::Store(StoreError::ScopeDenied { .. }) => StatusCode::FORBIDDEN,
            // A caller cannot confirm another actor's memory exists (the
            // store's own design), so a missing or unreadable id is a 404.
            GatewayError::Store(StoreError::NotFound(_)) => StatusCode::NOT_FOUND,
            GatewayError::Store(StoreError::Lifecycle(_)) => StatusCode::CONFLICT,
            GatewayError::Store(
                StoreError::EmbeddingDimensions { .. }
                | StoreError::Row(_)
                | StoreError::Embedding(_)
                | StoreError::Database(_),
            ) => StatusCode::INTERNAL_SERVER_ERROR,
            // StoreError is #[non_exhaustive]: a future variant this gateway
            // has not been taught about yet is a server-side gap, not a
            // client mistake.
            GatewayError::Store(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorBody {
                error: self.to_string(),
            }),
        )
            .into_response()
    }
}
