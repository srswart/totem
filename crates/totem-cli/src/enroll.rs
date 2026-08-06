//! `totem enroll`'s registration step: parse a repo's `/arrive/` tree and
//! send the resulting landscape snapshot to a running gateway's `POST
//! /enroll` (docs/solution-intent.md §3.3, §2.3; ADV-CLI-001).
//!
//! The CLI is a separate process from the gateway, so this crosses a real
//! network boundary rather than opening its own store connection — the
//! gateway (not the CLI) owns the store.

use std::path::Path;

use serde::Deserialize;
use thiserror::Error;
use totem_arrive_sync::IngestError;

/// What one `totem enroll` run wrote, as reported by the gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct EnrollSummary {
    /// Systems written.
    pub systems: usize,
    /// Components written.
    pub components: usize,
    /// Advances written.
    pub advances: usize,
}

/// Why `enroll` could not complete.
#[derive(Debug, Error)]
pub enum EnrollError {
    /// `/arrive/` could not be parsed.
    #[error("reading /arrive/: {0}")]
    Ingest(#[from] IngestError),
    /// The snapshot could not be encoded as JSON.
    #[error("encoding the landscape snapshot: {0}")]
    Encode(#[source] serde_json::Error),
    /// The gateway could not be reached, or its response could not be read.
    #[error("calling the gateway at {url}: {source}")]
    Request {
        /// The `/enroll` URL that was called.
        url: String,
        /// The underlying transport failure.
        #[source]
        source: reqwest::Error,
    },
    /// The gateway responded, but refused the enrollment.
    #[error("the gateway refused the enrollment ({status}): {body}")]
    Refused {
        /// The HTTP status the gateway returned.
        status: reqwest::StatusCode,
        /// The response body, for a human to read.
        body: String,
    },
}

/// Parse `arrive_root`'s `/arrive/` tree and POST the resulting landscape
/// snapshot to `<gateway_url>/enroll`, tagging the sync run with `source`
/// (e.g. `"cli:enroll"`, `"hook:post-commit"`).
pub async fn enroll(
    client: &reqwest::Client,
    gateway_url: &str,
    arrive_root: &Path,
    source: &str,
) -> Result<EnrollSummary, EnrollError> {
    let snapshot = totem_arrive_sync::read_repo_artifacts(arrive_root)?;

    let mut body = serde_json::to_value(&snapshot).map_err(EnrollError::Encode)?;
    body.as_object_mut()
        .expect("a LandscapeSnapshot serialises to a JSON object")
        .insert(
            "source".to_string(),
            serde_json::Value::String(source.to_string()),
        );

    let url = format!("{}/enroll", gateway_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|source| EnrollError::Request {
            url: url.clone(),
            source,
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(EnrollError::Refused { status, body });
    }

    response
        .json::<EnrollSummary>()
        .await
        .map_err(|source| EnrollError::Request { url, source })
}
