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
use axum::http::header::WWW_AUTHENTICATE;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use totem_store::StoreError;

use crate::auth::AuthError;

/// Why a request was refused, after it parsed successfully.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// The store refused the operation.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A request field parsed as JSON but failed a rule `totem-core`'s own
    /// types could not enforce at deserialization time (e.g. an empty
    /// `advance_id` — [`totem_core::SubjectRef::new`]'s validation, run
    /// inside a handler rather than a field's own `Deserialize`, since
    /// `SubjectRef` takes its kind from context, not the wire format).
    #[error("{0}")]
    InvalidRequest(String),
    /// The caller presented no usable credential, or asked for something
    /// outside the one they presented (ADV-GATEWAY-003).
    #[error(transparent)]
    Auth(#[from] AuthError),
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
            // A promotion or curation policy refused the operation, or the
            // caller asked to decide/roll back something already decided —
            // a rule the caller can act on, same footing as `Lifecycle`.
            GatewayError::Store(
                StoreError::Promotion(_)
                | StoreError::PromotionDecided(_)
                | StoreError::Curation(_)
                | StoreError::CurationRolledBack(_)
                | StoreError::Governance(_)
                | StoreError::ReviewDecided(_),
            ) => StatusCode::CONFLICT,
            // Same leak concern as `NotFound`: a proposal or curation event
            // this caller cannot reach must not be distinguishable from one
            // that never existed.
            GatewayError::Store(
                StoreError::PromotionNotFound(_) | StoreError::CurationNotFound(_),
            ) => StatusCode::NOT_FOUND,
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
            // Named the same way a `StoreError` client refusal is: a rule the
            // caller can act on, so its message is safe to return verbatim.
            GatewayError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            // "We do not know who you are" and "you are outside your grant"
            // are different answers, and a client that cannot tell them apart
            // retries a revoked credential forever instead of renewing it.
            GatewayError::Auth(error) if error.is_authentication_failure() => {
                StatusCode::UNAUTHORIZED
            }
            GatewayError::Auth(_) => StatusCode::FORBIDDEN,
        };
        // A 4xx/409 message is safe to return verbatim — it names a rule the
        // caller can act on (a denied scope, a missing record). A 5xx message
        // wraps a StoreError::Row/Embedding/Database detail that was never
        // meant for a client — a decode failure or a database error string —
        // so it is replaced with a generic message instead of forwarded.
        let message = if status.is_server_error() {
            "internal error".to_string()
        } else {
            self.to_string()
        };

        let mut response = (status, Json(ErrorBody { error: message })).into_response();
        if status == StatusCode::UNAUTHORIZED {
            // Both verified cloud harnesses discover how to authenticate from
            // this header (docs/tech-direction/mcp.md MCP-003/MCP-004); a bare
            // 401 leaves a client with nothing to act on.
            response
                .headers_mut()
                .insert(WWW_AUTHENTICATE, "Bearer".parse().expect("static header"));
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use totem_core::MemoryId;

    use super::*;

    async fn body_string(response: Response) -> String {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body collects")
            .to_bytes();
        String::from_utf8(bytes.to_vec()).expect("body is utf-8")
    }

    #[tokio::test]
    async fn a_server_error_never_forwards_the_underlying_detail() {
        let error = GatewayError::Store(StoreError::Row("secret internal detail".to_string()));
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = body_string(response).await;
        assert!(!body.contains("secret internal detail"), "leaked: {body}");
        assert!(body.contains("internal error"), "got: {body}");
    }

    #[tokio::test]
    async fn a_client_error_still_names_the_rule_it_violated() {
        let error = GatewayError::Store(StoreError::NotFound(MemoryId::new()));
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = body_string(response).await;
        assert!(
            body.contains("is not present in the caller's scope chain"),
            "got: {body}"
        );
    }
}
