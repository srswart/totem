//! The wasm-only pieces: fetching the gateway's REST surface from the
//! browser, and the root component that wires fetched data into [`crate::app::App`].
//!
//! Everything here is untested by unit test, deliberately and for the same
//! reason `crates/totem-gateway/src/main.rs` and `bin/mcp_stdio.rs` are:
//! it is thin wiring over already-tested logic (`crate::view_model`'s
//! parsing, `crate::app`'s rendering) plus a real network call, which a
//! `cargo test` run in this sandbox cannot exercise (no browser, no gateway
//! listening). Built only for `wasm32-unknown-unknown` — see `lib.rs`'s
//! `#[cfg(target_arch = "wasm32")]` on this module and `Cargo.toml`'s
//! per-target `dioxus-web`/`gloo-net` dependencies.
//!
//! No live-query wiring yet (TD-009: the console must consume gateway
//! events rather than open its own SurrealDB connection) — this is a
//! documented gap, not an oversight; see `ADV-CONSOLE-001.md`'s Risk +
//! Rollback section. Refresh here is manual (a button), not automatic.

use dioxus::prelude::*;
use gloo_net::http::Request;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use totem_core::MemoryRecord;

use crate::app::App;
use crate::view_model::{LandscapeViewModel, ViewModelError, parse_landscape, parse_memories};

/// `POST /recall`'s cap on this browser's own requests, in the absence of
/// any pagination UI (review feedback on PR #22: an unbounded `limit` risks
/// a very large response, and slow re-renders, once a project accumulates
/// records).
const RECALL_LIMIT: usize = 200;

/// Why a gateway fetch failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    /// The HTTP request itself failed (network, DNS, CORS).
    #[error("request failed: {0}")]
    Request(String),
    /// The gateway answered, but not with a 2xx status.
    #[error("gateway returned {status}: {body}")]
    Status {
        /// The HTTP status code.
        status: u16,
        /// The response body, if any.
        body: String,
    },
    /// The response body did not parse into the expected view model.
    #[error(transparent)]
    ViewModel(#[from] ViewModelError),
}

/// `GET /landscape/:repo`.
pub async fn fetch_landscape(repo: &str) -> Result<LandscapeViewModel, FetchError> {
    let repo = utf8_percent_encode(repo.trim(), NON_ALPHANUMERIC).to_string();
    let response = Request::get(&format!("/landscape/{repo}"))
        .send()
        .await
        .map_err(|error| FetchError::Request(error.to_string()))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| FetchError::Request(error.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(FetchError::Status { status, body });
    }
    Ok(parse_landscape(&body)?)
}

/// `POST /recall`, scoped to one actor's readable chain within one project.
pub async fn fetch_memories(actor: &str, project: &str) -> Result<Vec<MemoryRecord>, FetchError> {
    let request_body = serde_json::json!({
        "actor": actor.trim(),
        "project": project.trim(),
        "teams": [],
        "query": null,
        "categories": [],
        "since": null,
        "limit": RECALL_LIMIT,
        "harness": "console",
        "session": "console-session",
        "turn": null,
    });
    let response = Request::post("/recall")
        .header("content-type", "application/json")
        .body(request_body.to_string())
        .map_err(|error| FetchError::Request(error.to_string()))?
        .send()
        .await
        .map_err(|error| FetchError::Request(error.to_string()))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| FetchError::Request(error.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(FetchError::Status { status, body });
    }
    Ok(parse_memories(&body)?)
}

/// The wasm entry point's root component: a repo/actor/project form over
/// [`App`], with a manual "Refresh" button in place of the live-query
/// auto-update the advance names as a stretch goal (see module docs).
#[component]
pub fn RootApp() -> Element {
    let mut repo = use_signal(|| "058-totem".to_string());
    let mut actor = use_signal(|| "".to_string());
    let mut project = use_signal(|| "".to_string());
    let error = use_signal(|| Option::<String>::None);
    let landscape = use_signal(LandscapeViewModel::default);
    let memories = use_signal(Vec::<MemoryRecord>::new);

    let refresh = move || {
        let repo_value = repo.read().clone();
        let actor_value = actor.read().clone();
        let project_value = project.read().clone();
        let mut landscape = landscape;
        let mut memories = memories;
        let mut error = error;
        spawn(async move {
            // Cleared up front (review feedback on PR #22): otherwise a
            // stale error from a previous failed refresh keeps showing next
            // to a now-successful result.
            error.set(None);
            match fetch_landscape(&repo_value).await {
                Ok(view) => landscape.set(view),
                Err(err) => error.set(Some(err.to_string())),
            }
            if actor_value.trim().is_empty() || project_value.trim().is_empty() {
                // Cleared, not left stale: an actor/project the form no
                // longer names should not keep showing that actor's memories.
                memories.set(Vec::new());
            } else {
                match fetch_memories(&actor_value, &project_value).await {
                    Ok(records) => memories.set(records),
                    Err(err) => error.set(Some(err.to_string())),
                }
            }
        });
    };

    rsx! {
        div { class: "totem-console__connect",
            label { "Repo "
                input {
                    value: "{repo}",
                    oninput: move |event| repo.set(event.value()),
                }
            }
            label { "Actor "
                input {
                    value: "{actor}",
                    oninput: move |event| actor.set(event.value()),
                }
            }
            label { "Project "
                input {
                    value: "{project}",
                    oninput: move |event| project.set(event.value()),
                }
            }
            button { onclick: move |_| refresh(), "Refresh" }
            if let Some(message) = error.read().as_ref() {
                p { class: "totem-console__error", "{message}" }
            }
        }
        App { landscape: landscape.read().clone(), memories: memories.read().clone() }
    }
}
