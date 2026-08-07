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
//! The landscape signal now also patches in place from the gateway's live
//! relay (`GET /landscape/:repo/events`, ADV-CONSOLE-003) via
//! [`subscribe_landscape_events`] — the gap `ADV-CONSOLE-001.md`'s Risk +
//! Rollback section named. Refresh remains a working manual fallback: if the
//! stream is unavailable (or the wasm32 `EventSource` API is absent — the
//! browser-verification-required part of this task the advance body notes
//! plainly), the console degrades to exactly the ADV-CONSOLE-001 behavior.

use dioxus::prelude::*;
use futures::StreamExt;
use gloo_net::eventsource::futures::EventSource;
use gloo_net::http::Request;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use totem_core::{MemoryId, MemoryRecord, PromotionEvent, PromotionId, ReviewState};

use crate::app::App;
use crate::view_model::{
    AuditTrailViewModel, LandscapeViewModel, ViewModelError, parse_audit_trail, parse_landscape,
    parse_landscape_event, parse_memories, parse_promotion_queue, parse_uncertainty_queue,
};

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

/// `POST` a JSON body and return the response text, or a [`FetchError`] for a
/// non-2xx status — the boilerplate every governance read/write below shares
/// (`fetch_landscape`/`fetch_memories` predate this helper and are left as
/// they are rather than churned for a pattern they only used once each).
async fn post_json(path: &str, body: serde_json::Value) -> Result<String, FetchError> {
    let response = Request::post(path)
        .header("content-type", "application/json")
        .body(body.to_string())
        .map_err(|error| FetchError::Request(error.to_string()))?
        .send()
        .await
        .map_err(|error| FetchError::Request(error.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| FetchError::Request(error.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(FetchError::Status { status, body: text });
    }
    Ok(text)
}

/// `POST /promotions/pending`, scoped to one actor's readable chain within
/// one project (ADV-CONSOLE-002).
pub async fn fetch_promotion_queue(
    actor: &str,
    project: &str,
) -> Result<Vec<PromotionEvent>, FetchError> {
    let body = serde_json::json!({
        "actor": actor.trim(),
        "project": project.trim(),
        "teams": [],
        "harness": "console",
        "session": "console-session",
        "turn": null,
    });
    Ok(parse_promotion_queue(
        &post_json("/promotions/pending", body).await?,
    )?)
}

/// `POST /promotions/:id/approve`, deciding as `actor` (ADV-CONSOLE-002).
pub async fn approve_promotion(
    actor: &str,
    project: &str,
    proposal: PromotionId,
) -> Result<(), FetchError> {
    decide_promotion(actor, project, proposal, "approve").await
}

/// `POST /promotions/:id/reject`, deciding as `actor` (ADV-CONSOLE-002).
pub async fn reject_promotion(
    actor: &str,
    project: &str,
    proposal: PromotionId,
) -> Result<(), FetchError> {
    decide_promotion(actor, project, proposal, "reject").await
}

async fn decide_promotion(
    actor: &str,
    project: &str,
    proposal: PromotionId,
    decision: &str,
) -> Result<(), FetchError> {
    let body = serde_json::json!({
        "project": project.trim(),
        "teams": [],
        "author": { "kind": "human", "actor": actor.trim() },
        "harness": "console",
        "session": "console-session",
        "turn": null,
        "reason": null,
    });
    post_json(&format!("/promotions/{proposal}/{decision}"), body).await?;
    Ok(())
}

/// `POST /uncertainty/pending`, scoped to one actor's readable chain within
/// one project (ADV-CONSOLE-002).
pub async fn fetch_uncertainty_queue(
    actor: &str,
    project: &str,
) -> Result<Vec<MemoryRecord>, FetchError> {
    let body = serde_json::json!({
        "actor": actor.trim(),
        "project": project.trim(),
        "teams": [],
        "harness": "console",
        "session": "console-session",
        "turn": null,
    });
    Ok(parse_uncertainty_queue(
        &post_json("/uncertainty/pending", body).await?,
    )?)
}

/// `POST /uncertainty/:id/resolve`, deciding as `actor` (ADV-CONSOLE-002).
pub async fn resolve_uncertainty(
    actor: &str,
    project: &str,
    memory_id: MemoryId,
    decision: ReviewState,
) -> Result<(), FetchError> {
    let body = serde_json::json!({
        "actor": actor.trim(),
        "project": project.trim(),
        "teams": [],
        "decision": serde_json::to_value(decision).expect("ReviewState serialises"),
        "harness": "console",
        "session": "console-session",
        "turn": null,
    });
    post_json(&format!("/uncertainty/{memory_id}/resolve"), body).await?;
    Ok(())
}

/// `POST /audit/:id`, scoped to one actor's readable chain within one
/// project (ADV-CONSOLE-002).
pub async fn fetch_audit_trail(
    actor: &str,
    project: &str,
    memory_id: &str,
) -> Result<AuditTrailViewModel, FetchError> {
    let memory_id = utf8_percent_encode(memory_id.trim(), NON_ALPHANUMERIC).to_string();
    let body = serde_json::json!({
        "actor": actor.trim(),
        "project": project.trim(),
        "teams": [],
        "harness": "console",
        "session": "console-session",
        "turn": null,
    });
    Ok(parse_audit_trail(
        &post_json(&format!("/audit/{memory_id}"), body).await?,
    )?)
}

/// Subscribe to the gateway's live landscape relay for `repo` and patch
/// `landscape` in place on every event, until the stream ends (the gateway
/// closed it, or a malformed payload broke the connection) or this task is
/// dropped (`use_future` cancels the previous run when `repo` changes, and
/// every run when the component unmounts — Dioxus 0.6's own cancellation
/// contract for the hook, not something this function manages itself).
///
/// `actor`/`session` are fixed rather than taken from the form: the relay's
/// own access-log entries only need a stable subscriber identity, the same
/// way `fetch_memories`'s `harness: "console"` is fixed rather than
/// caller-supplied.
async fn subscribe_landscape_events(repo: &str, landscape: &mut Signal<LandscapeViewModel>) {
    let encoded = utf8_percent_encode(repo.trim(), NON_ALPHANUMERIC).to_string();
    let url = format!("/landscape/{encoded}/events?actor=console&session=console-session");
    let Ok(mut source) = EventSource::new(&url) else {
        return;
    };
    let Ok(mut stream) = source.subscribe("landscape") else {
        return;
    };
    while let Some(Ok((_event_type, message))) = stream.next().await {
        let Some(data) = message.data().as_string() else {
            continue;
        };
        if let Ok(view) = parse_landscape_event(&data) {
            landscape.set(view);
        }
    }
}

/// The wasm entry point's root component: a repo/actor/project form over
/// [`App`]. The landscape section updates live via
/// [`subscribe_landscape_events`] (ADV-CONSOLE-003); the manual "Refresh"
/// button remains as a fallback and is still the only update path for
/// memories/promotions/uncertainty (ADV-CONSOLE-003's scope is the
/// dashboard's landscape section — see the advance body).
#[component]
pub fn RootApp() -> Element {
    let mut repo = use_signal(|| "058-totem".to_string());
    let mut actor = use_signal(|| "".to_string());
    let mut project = use_signal(|| "".to_string());
    let mut audit_query = use_signal(|| "".to_string());
    let error = use_signal(|| Option::<String>::None);
    let landscape = use_signal(LandscapeViewModel::default);
    let memories = use_signal(Vec::<MemoryRecord>::new);
    let promotions = use_signal(Vec::<PromotionEvent>::new);
    let uncertainty = use_signal(Vec::<MemoryRecord>::new);
    let audit = use_signal(|| Option::<AuditTrailViewModel>::None);

    // Live landscape updates (ADV-CONSOLE-003): re-subscribes whenever
    // `repo` changes, and `use_future` cancels the previous subscription's
    // task for us — no manual EventSource lifecycle management here.
    use_future(move || {
        let repo_value = repo.read().clone();
        let mut landscape = landscape;
        async move {
            subscribe_landscape_events(&repo_value, &mut landscape).await;
        }
    });

    let refresh = move || {
        let repo_value = repo.read().clone();
        let actor_value = actor.read().clone();
        let project_value = project.read().clone();
        let mut landscape = landscape;
        let mut memories = memories;
        let mut promotions = promotions;
        let mut uncertainty = uncertainty;
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
                promotions.set(Vec::new());
                uncertainty.set(Vec::new());
            } else {
                match fetch_memories(&actor_value, &project_value).await {
                    Ok(records) => memories.set(records),
                    Err(err) => error.set(Some(err.to_string())),
                }
                match fetch_promotion_queue(&actor_value, &project_value).await {
                    Ok(events) => promotions.set(events),
                    Err(err) => error.set(Some(err.to_string())),
                }
                match fetch_uncertainty_queue(&actor_value, &project_value).await {
                    Ok(records) => uncertainty.set(records),
                    Err(err) => error.set(Some(err.to_string())),
                }
            }
        });
    };

    let lookup_audit = move || {
        let actor_value = actor.read().clone();
        let project_value = project.read().clone();
        let query_value = audit_query.read().clone();
        let mut audit = audit;
        let mut error = error;
        spawn(async move {
            error.set(None);
            match fetch_audit_trail(&actor_value, &project_value, &query_value).await {
                Ok(trail) => audit.set(Some(trail)),
                Err(err) => error.set(Some(err.to_string())),
            }
        });
    };

    let on_approve_promotion = move |proposal: PromotionId| {
        let actor_value = actor.read().clone();
        let project_value = project.read().clone();
        let mut error = error;
        spawn(async move {
            if let Err(err) = approve_promotion(&actor_value, &project_value, proposal).await {
                error.set(Some(err.to_string()));
            }
            refresh();
        });
    };

    let on_reject_promotion = move |proposal: PromotionId| {
        let actor_value = actor.read().clone();
        let project_value = project.read().clone();
        let mut error = error;
        spawn(async move {
            if let Err(err) = reject_promotion(&actor_value, &project_value, proposal).await {
                error.set(Some(err.to_string()));
            }
            refresh();
        });
    };

    let on_resolve_uncertainty = move |(memory_id, decision): (MemoryId, ReviewState)| {
        let actor_value = actor.read().clone();
        let project_value = project.read().clone();
        let mut error = error;
        spawn(async move {
            if let Err(err) =
                resolve_uncertainty(&actor_value, &project_value, memory_id, decision).await
            {
                error.set(Some(err.to_string()));
            }
            refresh();
        });
    };

    let field = "rounded-md border border-slate-300 bg-white px-2.5 py-1.5 text-sm text-slate-900 shadow-sm focus:border-indigo-500 focus:outline-none";
    let label_cls = "flex items-center gap-2 text-xs font-medium text-slate-500";

    rsx! {
        // Inlined rather than linked: dx serves unknown paths as the SPA
        // fallback, and the asset!() pipeline varies across dx versions —
        // include_str! of the committed build works everywhere (16KB).
        style { dangerous_inner_html: include_str!("../assets/tailwind.css") }
        div { class: "min-h-screen bg-slate-50 text-slate-900 antialiased",
        header { class: "totem-console__connect mb-8 border-b border-slate-200 bg-white shadow-sm",
            div { class: "mx-auto flex max-w-5xl flex-wrap items-center gap-x-5 gap-y-3 px-6 py-4",
                span { class: "mr-2 text-lg font-semibold tracking-tight text-indigo-700", "Totem" }
                label { class: label_cls, "Repo"
                    input {
                        class: field,
                        value: "{repo}",
                        oninput: move |event| repo.set(event.value()),
                    }
                }
                label { class: label_cls, "Actor"
                    input {
                        class: field,
                        value: "{actor}",
                        oninput: move |event| actor.set(event.value()),
                    }
                }
                label { class: label_cls, "Project"
                    input {
                        class: field,
                        value: "{project}",
                        oninput: move |event| project.set(event.value()),
                    }
                }
                button {
                    class: "rounded-md bg-indigo-600 px-3.5 py-1.5 text-sm font-medium text-white shadow-sm hover:bg-indigo-700",
                    onclick: move |_| refresh(),
                    "Refresh"
                }
            }
            div { class: "totem-console__audit-lookup mx-auto flex max-w-5xl flex-wrap items-center gap-x-5 gap-y-3 px-6 pb-4",
                label { class: label_cls, "Memory id"
                    input {
                        class: "{field} w-80 font-mono",
                        value: "{audit_query}",
                        oninput: move |event| audit_query.set(event.value()),
                    }
                }
                button {
                    class: "rounded-md border border-slate-300 bg-white px-3.5 py-1.5 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-100",
                    onclick: move |_| lookup_audit(),
                    "Look up audit trail"
                }
            }
            if let Some(message) = error.read().as_ref() {
                p { class: "totem-console__error mx-auto max-w-5xl px-6 pb-4 text-sm font-medium text-rose-700",
                    "{message}"
                }
            }
        }
        App {
            landscape: landscape.read().clone(),
            memories: memories.read().clone(),
            promotions: promotions.read().clone(),
            on_approve_promotion,
            on_reject_promotion,
            uncertainty: uncertainty.read().clone(),
            on_resolve_uncertainty,
            audit: audit.read().clone(),
        }
        }
    }
}
