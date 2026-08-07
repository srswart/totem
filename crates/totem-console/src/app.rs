//! The console's Dioxus component tree: a two-tab shell over the landscape
//! dashboard and the memory browser (docs/solution-intent.md §5, G5 —
//! "humans observe"). Thin over [`crate::view_model`]: every rendering
//! decision here reads already-parsed, already-grouped data — the
//! JSON-to-view-model boundary is shared and tested in `view_model.rs`, not
//! duplicated as inline `serde_json` calls inside a component.
//!
//! `App` takes its data as props rather than fetching it itself, so it (and
//! every view under it) renders identically under `dioxus-ssr` on the
//! native host and under `dioxus-web` in a browser — `src/api.rs`
//! (wasm32-only) owns fetching and re-rendering on refresh.

use dioxus::prelude::*;
use totem_core::{MemoryId, MemoryRecord, PromotionEvent, PromotionId, ReviewState};

use crate::view_model::{
    AdvanceView, AuditTrailViewModel, ComponentView, LandscapeViewModel, group_by_category,
};

/// Which tab the console is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// The landscape dashboard.
    Landscape,
    /// The memory browser.
    Memories,
    /// The promotion-approval and Uncertainty-resolution queues
    /// (ADV-CONSOLE-002).
    Governance,
    /// The audit trail viewer (ADV-CONSOLE-002).
    Audit,
}

/// Root component: a tab switcher over the four views. The three governance
/// views ([`PromotionQueueView`], [`UncertaintyQueueView`], [`AuditTrailView`])
/// take their action callbacks as [`EventHandler`] props rather than
/// fetching themselves, the same "data and callbacks in, no network calls"
/// discipline this module's doc comment already applies to `App` as a whole
/// — `src/api.rs`'s `RootApp` is the one place that wires a callback to an
/// actual gateway request and a refresh.
#[component]
pub fn App(
    landscape: LandscapeViewModel,
    memories: Vec<MemoryRecord>,
    promotions: Vec<PromotionEvent>,
    on_approve_promotion: EventHandler<PromotionId>,
    on_reject_promotion: EventHandler<PromotionId>,
    uncertainty: Vec<MemoryRecord>,
    on_resolve_uncertainty: EventHandler<(MemoryId, ReviewState)>,
    audit: Option<AuditTrailViewModel>,
) -> Element {
    let mut tab = use_signal(|| Tab::Landscape);

    let tab_class = |own: Tab, current: Tab| {
        if own == current {
            "tab tab--active border-indigo-600 text-indigo-700 border-b-2 px-4 py-2 text-sm font-medium"
        } else {
            "tab border-transparent text-slate-500 hover:text-slate-800 border-b-2 px-4 py-2 text-sm font-medium"
        }
    };

    rsx! {
        div { class: "totem-console totem-shell mx-auto max-w-5xl px-6 pb-16",
            nav { class: "totem-console__tabs mb-6 flex gap-1 border-b border-slate-200",
                button {
                    class: tab_class(Tab::Landscape, *tab.read()),
                    onclick: move |_| tab.set(Tab::Landscape),
                    "Landscape"
                }
                button {
                    class: tab_class(Tab::Memories, *tab.read()),
                    onclick: move |_| tab.set(Tab::Memories),
                    "Memories"
                }
                button {
                    class: tab_class(Tab::Governance, *tab.read()),
                    onclick: move |_| tab.set(Tab::Governance),
                    "Governance"
                }
                button {
                    class: tab_class(Tab::Audit, *tab.read()),
                    onclick: move |_| tab.set(Tab::Audit),
                    "Audit"
                }
            }
            main { class: "space-y-8",
                match *tab.read() {
                    Tab::Landscape => rsx! { LandscapeView { view: landscape.clone() } },
                    Tab::Memories => rsx! { MemoryBrowserView { records: memories.clone() } },
                    Tab::Governance => rsx! {
                        PromotionQueueView {
                            promotions: promotions.clone(),
                            on_approve: on_approve_promotion,
                            on_reject: on_reject_promotion,
                        }
                        UncertaintyQueueView {
                            records: uncertainty.clone(),
                            on_resolve: on_resolve_uncertainty,
                        }
                    },
                    Tab::Audit => rsx! { AuditTrailView { audit: audit.clone() } },
                }
            }
        }
    }
}

/// The promotion-approval queue: open proposals aimed at a scope the reader
/// can reach, with an approve/reject action per row (ADV-CONSOLE-002 —
/// "human-gated promotions are approved or rejected in the console").
#[component]
pub fn PromotionQueueView(
    promotions: Vec<PromotionEvent>,
    on_approve: EventHandler<PromotionId>,
    on_reject: EventHandler<PromotionId>,
) -> Element {
    rsx! {
        section { class: "promotion-queue space-y-3",
            h3 { class: "text-xs font-semibold uppercase tracking-wider text-slate-400", "Promotions awaiting approval" }
            if promotions.is_empty() {
                p { class: "empty rounded-lg border border-dashed border-slate-300 bg-slate-50 px-4 py-6 text-center text-sm text-slate-500",
                    "No promotions are waiting on a decision."
                }
            }
            ul { class: "space-y-1.5",
                for proposal in promotions.iter() {
                    li {
                        key: "{proposal.id}",
                        class: "flex flex-wrap items-center gap-3 rounded-lg border border-slate-200 bg-white px-4 py-2.5 shadow-sm",
                        span { class: "promotion-move flex-1 font-mono text-xs text-slate-700",
                            "{proposal.memory}: {proposal.from_scope} \u{2192} {proposal.to_scope}"
                        }
                        {
                            let id = proposal.id;
                            rsx! {
                                button {
                                    class: "rounded-md bg-emerald-600 px-3 py-1 text-xs font-medium text-white hover:bg-emerald-700",
                                    onclick: move |_| on_approve.call(id),
                                    "Approve"
                                }
                                button {
                                    class: "rounded-md border border-rose-300 px-3 py-1 text-xs font-medium text-rose-700 hover:bg-rose-50",
                                    onclick: move |_| on_reject.call(id),
                                    "Reject"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The Uncertainty queue: contested memories awaiting a human's resolution,
/// with an approve/reject action per row (ADV-CONSOLE-002 — "contested
/// memories sit in a queue until a human resolves them").
#[component]
pub fn UncertaintyQueueView(
    records: Vec<MemoryRecord>,
    on_resolve: EventHandler<(MemoryId, ReviewState)>,
) -> Element {
    rsx! {
        section { class: "uncertainty-queue space-y-3",
            h3 { class: "text-xs font-semibold uppercase tracking-wider text-slate-400", "Uncertainty awaiting resolution" }
            if records.is_empty() {
                p { class: "empty rounded-lg border border-dashed border-slate-300 bg-slate-50 px-4 py-6 text-center text-sm text-slate-500",
                    "No contested memories are waiting on a decision."
                }
            }
            ul { class: "space-y-1.5",
                for record in records.iter() {
                    li {
                        key: "{record.id}",
                        class: "flex flex-wrap items-center gap-3 rounded-lg border border-slate-200 bg-white px-4 py-2.5 shadow-sm",
                        span { class: "scope badge bg-slate-100 text-slate-600 inline-flex items-center rounded-full px-2 py-0.5 font-mono text-xs",
                            "{record.scope}"
                        }
                        span { class: "flex-1 text-sm text-slate-900", "{record.content.body}" }
                        {
                            let id = record.id;
                            rsx! {
                                button {
                                    class: "rounded-md bg-emerald-600 px-3 py-1 text-xs font-medium text-white hover:bg-emerald-700",
                                    onclick: move |_| on_resolve.call((id, ReviewState::Approved)),
                                    "Approve"
                                }
                                button {
                                    class: "rounded-md border border-rose-300 px-3 py-1 text-xs font-medium text-rose-700 hover:bg-rose-50",
                                    onclick: move |_| on_resolve.call((id, ReviewState::Rejected)),
                                    "Reject"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The audit trail viewer: one memory's provenance, access history, curator
/// lineage, and promotion history on demand (ADV-CONSOLE-002 — "a reviewer
/// can reconstruct any memory's lineage from the console").
#[component]
pub fn AuditTrailView(audit: Option<AuditTrailViewModel>) -> Element {
    let Some(audit) = audit else {
        return rsx! {
            section { class: "audit-trail",
                p { class: "empty rounded-lg border border-dashed border-slate-300 bg-slate-50 px-4 py-6 text-center text-sm text-slate-500",
                    "Look up a memory id to see its audit trail."
                }
            }
        };
    };

    rsx! {
        section { class: "audit-trail space-y-6",
            div { class: "rounded-lg border border-slate-200 bg-white p-4 shadow-sm",
                h3 { class: "mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400", "Record" }
                p { class: "text-sm text-slate-900", "{audit.record.content.body}" }
                p { class: "provenance mt-2 text-xs text-slate-400",
                    "written by {audit.record.provenance.author.actor()} via {audit.record.provenance.harness:?} "
                    "at {audit.record.provenance.created_at}"
                }
            }
            div {
                h3 { class: "mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400", "Access history" }
                if audit.access_log.is_empty() {
                    p { class: "empty rounded-lg border border-dashed border-slate-300 bg-slate-50 px-4 py-4 text-center text-sm text-slate-500",
                        "No logged reads or writes yet."
                    }
                }
                ul { class: "space-y-1",
                    for entry in audit.access_log.iter() {
                        li {
                            key: "{entry.at}",
                            class: "rounded-md border border-slate-100 bg-white px-3 py-1.5 font-mono text-xs text-slate-600",
                            "{entry.operation:?} via {entry.endpoint} at {entry.at}"
                        }
                    }
                }
            }
            div {
                h3 { class: "mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400", "Curator lineage" }
                if audit.curation_history.is_empty() {
                    p { class: "empty rounded-lg border border-dashed border-slate-300 bg-slate-50 px-4 py-4 text-center text-sm text-slate-500",
                        "No curator actions yet."
                    }
                }
                ul { class: "space-y-1",
                    for event in audit.curation_history.iter() {
                        li {
                            key: "{event.id}",
                            class: "rounded-md border border-slate-100 bg-white px-3 py-1.5 font-mono text-xs text-slate-600",
                            "{event.kind:?} at {event.provenance.created_at}"
                        }
                    }
                }
            }
            div {
                h3 { class: "mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400", "Promotion history" }
                if audit.promotion_history.is_empty() {
                    p { class: "empty rounded-lg border border-dashed border-slate-300 bg-slate-50 px-4 py-4 text-center text-sm text-slate-500",
                        "This record has never changed scope."
                    }
                }
                ul { class: "space-y-1",
                    for event in audit.promotion_history.iter() {
                        li {
                            key: "{event.id}",
                            class: "rounded-md border border-slate-100 bg-white px-3 py-1.5 font-mono text-xs text-slate-600",
                            "{event.kind:?}: {event.from_scope} \u{2192} {event.to_scope}"
                        }
                    }
                }
            }
        }
    }
}

/// The landscape dashboard: systems, components, and advances for one repo
/// (Solution Intent §2.3, G2).
#[component]
pub fn LandscapeView(view: LandscapeViewModel) -> Element {
    rsx! {
        section { class: "landscape space-y-6",
            if let Some(repo) = &view.repo {
                h2 { class: "text-2xl font-semibold tracking-tight text-slate-900", "{repo.name}" }
            } else {
                p { class: "empty rounded-lg border border-dashed border-slate-300 bg-slate-50 px-4 py-6 text-center text-sm text-slate-500",
                    "This repo has not been synced yet."
                }
            }
            div {
                h3 { class: "mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400", "Systems" }
                ul { class: "space-y-1.5",
                    for system in view.systems.iter() {
                        li {
                            key: "{system.id}",
                            class: "rounded-lg border border-slate-200 bg-white px-4 py-2.5 text-sm text-slate-900 shadow-sm",
                            "{system.name}"
                        }
                    }
                }
            }
            div {
                h3 { class: "mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400", "Components" }
                ul { class: "space-y-1.5",
                    for component in view.components.iter() {
                        ComponentRow { component: component.clone() }
                    }
                }
            }
            div {
                h3 { class: "mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400", "Advances" }
                ul { class: "space-y-1.5",
                    for advance in view.advances.iter() {
                        AdvanceRow { advance: advance.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn ComponentRow(component: ComponentView) -> Element {
    let owners = component.owners.join(", ");
    let stage = component
        .stage
        .clone()
        .unwrap_or_else(|| "unstaged".to_string());

    rsx! {
        li {
            key: "{component.id}",
            class: "flex flex-wrap items-baseline gap-x-2 rounded-lg border border-slate-200 bg-white px-4 py-2.5 shadow-sm",
            strong { class: "font-mono text-xs text-slate-500", "{component.id}" }
            span { class: "text-sm text-slate-900", "{component.name}" }
            span { class: "badge badge--stage bg-sky-100 text-sky-800 inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
                "{stage}"
            }
            if !owners.is_empty() {
                span { class: "owners ml-auto text-xs text-slate-400", "owners: {owners}" }
            }
        }
    }
}

#[component]
fn AdvanceRow(advance: AdvanceView) -> Element {
    let status = advance
        .status
        .clone()
        .unwrap_or_else(|| "planned".to_string());
    let components = advance.components.join(", ");

    rsx! {
        li {
            key: "{advance.id}",
            class: "flex flex-wrap items-baseline gap-x-2 gap-y-1 rounded-lg border border-slate-200 bg-white px-4 py-2.5 shadow-sm",
            strong { class: "font-mono text-xs text-slate-500", "{advance.id}" }
            span { class: "text-sm text-slate-900", "{advance.title}" }
            StatusBadge { status: status.clone() }
            if !components.is_empty() {
                span { class: "components ml-auto text-xs text-slate-400", "impacts: {components}" }
            }
        }
    }
}

/// One advance status as a colored pill. Semantic classes (`badge`,
/// `badge--<status>`) are the tested surface; utilities are the styling.
#[component]
fn StatusBadge(status: String) -> Element {
    let color = match status.as_str() {
        "complete" | "done" => "bg-emerald-100 text-emerald-800",
        "in_progress" => "bg-amber-100 text-amber-800",
        "blocked" => "bg-rose-100 text-rose-800",
        _ => "bg-slate-100 text-slate-600",
    };
    rsx! {
        span { class: "badge badge--{status} {color} inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
            "{status}"
        }
    }
}

/// The memory browser: recalled records grouped by category (Solution
/// Intent §5: "browse memories by scope and category").
#[component]
pub fn MemoryBrowserView(records: Vec<MemoryRecord>) -> Element {
    let grouped = group_by_category(&records);
    let groups: Vec<(totem_core::MemoryCategory, Vec<MemoryRecord>)> = grouped
        .into_iter()
        .map(|(category, records)| (category, records.into_iter().cloned().collect()))
        .collect();

    rsx! {
        section { class: "memory-browser space-y-6",
            if records.is_empty() {
                p { class: "empty rounded-lg border border-dashed border-slate-300 bg-slate-50 px-4 py-6 text-center text-sm text-slate-500",
                    "No memories in view for this scope chain."
                }
            }
            for (category, records) in groups {
                CategoryGroup { category, records }
            }
        }
    }
}

#[component]
fn CategoryGroup(category: totem_core::MemoryCategory, records: Vec<MemoryRecord>) -> Element {
    let label = format!("{category:?}");

    rsx! {
        div { class: "category-group",
            h3 { class: "mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400", "{label}" }
            ul { class: "space-y-1.5",
                for record in records.iter() {
                    li {
                        key: "{record.id}",
                        class: "flex flex-wrap items-baseline gap-2 rounded-lg border border-slate-200 bg-white px-4 py-2.5 shadow-sm",
                        span { class: "scope badge bg-slate-100 text-slate-600 inline-flex items-center rounded-full px-2 py-0.5 font-mono text-xs",
                            "{record.scope}"
                        }
                        span { class: "text-sm text-slate-900", "{record.content.body}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use totem_core::{
        ActorId, Author, Content, Harness, MemoryCategory, Provenance, Scope, SessionId,
    };

    use super::*;
    use crate::view_model::{ComponentView, LandscapeViewModel, RepoView, SystemView};

    fn ssr(vdom: &mut VirtualDom) -> String {
        vdom.rebuild_in_place();
        dioxus_ssr::render(vdom)
    }

    fn a_memory_record(category: MemoryCategory, body: &str) -> MemoryRecord {
        MemoryRecord::new(
            category,
            Scope::Actor(ActorId::new("ada").expect("valid actor id")),
            Content::new(body),
            Provenance::new(
                Author::Human(ActorId::new("ada").expect("valid actor id")),
                Harness::Console,
                SessionId::new("sess-1").expect("valid session id"),
                Utc::now(),
            ),
        )
    }

    fn synced_landscape() -> LandscapeViewModel {
        LandscapeViewModel {
            repo: Some(RepoView {
                id: "058-totem".to_string(),
                name: "Totem".to_string(),
            }),
            systems: vec![SystemView {
                id: "058-totem-core".to_string(),
                name: "Totem Core".to_string(),
            }],
            components: vec![ComponentView {
                id: "console".to_string(),
                system: "058-totem-core".to_string(),
                name: "Totem Console".to_string(),
                stage: Some("incubating".to_string()),
                owners: vec!["058-totem".to_string()],
            }],
            advances: Vec::new(),
        }
    }

    #[test]
    fn advance_status_renders_as_a_semantic_status_badge() {
        let mut vdom = VirtualDom::new_with_props(
            AdvanceRow,
            AdvanceRowProps {
                advance: AdvanceView {
                    id: "ADV-CONSOLE-004".to_string(),
                    system: "058-totem-core".to_string(),
                    title: "Console visual design system".to_string(),
                    status: Some("complete".to_string()),
                    components: vec!["console".to_string()],
                },
            },
        );
        let html = ssr(&mut vdom);
        assert!(
            html.contains("badge--complete"),
            "status badge class missing: {html}"
        );
        assert!(html.contains("badge"), "badge base class missing: {html}");
    }

    #[test]
    fn component_stage_renders_as_a_semantic_stage_badge() {
        let mut vdom = VirtualDom::new_with_props(
            ComponentRow,
            ComponentRowProps {
                component: ComponentView {
                    id: "console".to_string(),
                    system: "058-totem-core".to_string(),
                    name: "Totem Console".to_string(),
                    stage: Some("incubating".to_string()),
                    owners: vec!["058-totem".to_string()],
                },
            },
        );
        let html = ssr(&mut vdom);
        assert!(
            html.contains("badge--stage"),
            "stage badge class missing: {html}"
        );
    }

    #[component]
    fn ShellFixture() -> Element {
        rsx! {
            App {
                landscape: synced_landscape(),
                memories: Vec::new(),
                promotions: Vec::new(),
                on_approve_promotion: |_| {},
                on_reject_promotion: |_| {},
                uncertainty: Vec::new(),
                on_resolve_uncertainty: |_| {},
                audit: None,
            }
        }
    }

    #[test]
    fn the_app_shell_renders_tab_semantics_with_an_active_marker() {
        let mut vdom = VirtualDom::new(ShellFixture);
        let html = ssr(&mut vdom);
        assert!(
            html.contains("totem-shell"),
            "shell landmark class missing: {html}"
        );
        assert!(
            html.contains("tab--active"),
            "active tab marker missing: {html}"
        );
    }

    #[test]
    fn empty_states_carry_the_semantic_empty_class() {
        let mut vdom = VirtualDom::new_with_props(
            MemoryBrowserView,
            MemoryBrowserViewProps {
                records: Vec::new(),
            },
        );
        let html = ssr(&mut vdom);
        assert!(
            html.contains(r#"class="empty"#),
            "empty-state class missing: {html}"
        );
    }

    #[test]
    fn the_landscape_dashboard_renders_the_synced_repo_s_name_and_components() {
        let mut vdom = VirtualDom::new_with_props(
            LandscapeView,
            LandscapeViewProps {
                view: synced_landscape(),
            },
        );
        let html = ssr(&mut vdom);

        assert!(html.contains("Totem"), "repo name missing: {html}");
        assert!(
            html.contains("Totem Console"),
            "component name missing: {html}"
        );
        assert!(
            html.contains("incubating"),
            "component stage missing: {html}"
        );
        assert!(html.contains("058-totem"), "owner missing: {html}");
    }

    #[test]
    fn the_landscape_dashboard_names_an_unsynced_repo_instead_of_erroring() {
        let mut vdom = VirtualDom::new_with_props(
            LandscapeView,
            LandscapeViewProps {
                view: LandscapeViewModel::default(),
            },
        );
        let html = ssr(&mut vdom);

        assert!(
            html.contains("has not been synced yet"),
            "expected the empty-landscape message: {html}"
        );
    }

    #[test]
    fn the_memory_browser_groups_records_by_category() {
        let records = vec![
            a_memory_record(
                MemoryCategory::Knowledge,
                "the store enforces scope isolation",
            ),
            a_memory_record(MemoryCategory::Instructions, "run cargo fmt before pushing"),
        ];
        let mut vdom =
            VirtualDom::new_with_props(MemoryBrowserView, MemoryBrowserViewProps { records });
        let html = ssr(&mut vdom);

        assert!(
            html.contains("Knowledge"),
            "category heading missing: {html}"
        );
        assert!(
            html.contains("Instructions"),
            "category heading missing: {html}"
        );
        assert!(
            html.contains("the store enforces scope isolation"),
            "body missing: {html}"
        );
        assert!(
            html.contains("run cargo fmt before pushing"),
            "body missing: {html}"
        );
    }

    #[test]
    fn the_memory_browser_names_an_empty_result_instead_of_rendering_nothing() {
        let mut vdom = VirtualDom::new_with_props(
            MemoryBrowserView,
            MemoryBrowserViewProps {
                records: Vec::new(),
            },
        );
        let html = ssr(&mut vdom);

        assert!(
            html.contains("No memories in view"),
            "expected the empty-recall message: {html}"
        );
    }

    /// `App` (and each governance view below) takes its action callbacks as
    /// [`EventHandler`] props, and [`EventHandler::new`]/`rsx!`'s callback
    /// sugar can only build one from inside a live component's render — not
    /// from plain test setup code before [`VirtualDom::new_with_props`] runs.
    /// A thin harness component is the real, supported way to hand a
    /// prop-driven child component a callback in a test, and it exercises
    /// the exact same `rsx!` closure syntax `src/api.rs`'s `RootApp` uses in
    /// production.
    #[component]
    fn AppHarness(landscape: LandscapeViewModel, memories: Vec<MemoryRecord>) -> Element {
        rsx! {
            App {
                landscape,
                memories,
                promotions: Vec::new(),
                on_approve_promotion: move |_id| {},
                on_reject_promotion: move |_id| {},
                uncertainty: Vec::new(),
                on_resolve_uncertainty: move |_decision| {},
                audit: None,
            }
        }
    }

    #[test]
    fn the_app_shell_starts_on_the_landscape_tab() {
        let mut vdom = VirtualDom::new_with_props(
            AppHarness,
            AppHarnessProps {
                landscape: synced_landscape(),
                memories: Vec::new(),
            },
        );
        let html = ssr(&mut vdom);

        assert!(
            html.contains("Totem Console"),
            "expected the landscape tab's content: {html}"
        );
        assert!(
            html.contains("Landscape"),
            "expected the Landscape tab button: {html}"
        );
        assert!(
            html.contains("Memories"),
            "expected the Memories tab button: {html}"
        );
        assert!(
            html.contains("Governance"),
            "expected the Governance tab button: {html}"
        );
        assert!(
            html.contains("Audit"),
            "expected the Audit tab button: {html}"
        );
    }

    fn a_promotion_event() -> PromotionEvent {
        PromotionEvent::propose(
            totem_core::MemoryId::new(),
            Scope::Actor(ActorId::new("ada").expect("valid actor id")),
            Scope::Project(totem_core::RepoId::new("srswart/totem").expect("valid repo id")),
            Provenance::new(
                Author::Human(ActorId::new("ada").expect("valid actor id")),
                Harness::Console,
                SessionId::new("sess-1").expect("valid session id"),
                Utc::now(),
            ),
        )
    }

    #[component]
    fn PromotionQueueHarness(promotions: Vec<PromotionEvent>) -> Element {
        rsx! {
            PromotionQueueView {
                promotions,
                on_approve: move |_id| {},
                on_reject: move |_id| {},
            }
        }
    }

    #[test]
    fn the_promotion_queue_lists_each_proposal_with_approve_and_reject_actions() {
        let proposal = a_promotion_event();
        let mut vdom = VirtualDom::new_with_props(
            PromotionQueueHarness,
            PromotionQueueHarnessProps {
                promotions: vec![proposal.clone()],
            },
        );
        let html = ssr(&mut vdom);

        assert!(
            html.contains(&proposal.memory.to_string()),
            "expected the proposed memory's id: {html}"
        );
        assert!(
            html.contains("Approve"),
            "expected an Approve button: {html}"
        );
        assert!(html.contains("Reject"), "expected a Reject button: {html}");
    }

    #[test]
    fn the_promotion_queue_names_an_empty_result_instead_of_rendering_nothing() {
        let mut vdom = VirtualDom::new_with_props(
            PromotionQueueHarness,
            PromotionQueueHarnessProps {
                promotions: Vec::new(),
            },
        );
        let html = ssr(&mut vdom);

        assert!(
            html.contains("No promotions are waiting"),
            "expected the empty-queue message: {html}"
        );
    }

    #[component]
    fn UncertaintyQueueHarness(records: Vec<MemoryRecord>) -> Element {
        rsx! {
            UncertaintyQueueView {
                records,
                on_resolve: move |_decision| {},
            }
        }
    }

    #[test]
    fn the_uncertainty_queue_lists_each_contested_record_with_resolution_actions() {
        let contested = a_memory_record(
            MemoryCategory::Uncertainty,
            "actually the deploy runs on Thursdays",
        );
        let mut vdom = VirtualDom::new_with_props(
            UncertaintyQueueHarness,
            UncertaintyQueueHarnessProps {
                records: vec![contested.clone()],
            },
        );
        let html = ssr(&mut vdom);

        assert!(
            html.contains("actually the deploy runs on Thursdays"),
            "expected the contested claim's body: {html}"
        );
        assert!(
            html.contains("Approve"),
            "expected an Approve button: {html}"
        );
        assert!(html.contains("Reject"), "expected a Reject button: {html}");
    }

    #[test]
    fn the_uncertainty_queue_names_an_empty_result_instead_of_rendering_nothing() {
        let mut vdom = VirtualDom::new_with_props(
            UncertaintyQueueHarness,
            UncertaintyQueueHarnessProps {
                records: Vec::new(),
            },
        );
        let html = ssr(&mut vdom);

        assert!(
            html.contains("No contested memories are waiting"),
            "expected the empty-queue message: {html}"
        );
    }

    #[test]
    fn the_audit_trail_view_prompts_for_a_lookup_when_nothing_has_been_fetched_yet() {
        let mut vdom =
            VirtualDom::new_with_props(AuditTrailView, AuditTrailViewProps { audit: None });
        let html = ssr(&mut vdom);

        assert!(
            html.contains("Look up a memory id"),
            "expected the no-lookup-yet prompt: {html}"
        );
    }

    #[test]
    fn the_audit_trail_view_renders_the_record_and_its_history_sections() {
        let record = a_memory_record(MemoryCategory::Knowledge, "a note worth auditing");
        let audit = AuditTrailViewModel {
            record: record.clone(),
            access_log: Vec::new(),
            curation_history: Vec::new(),
            promotion_history: vec![a_promotion_event()],
        };
        let mut vdom =
            VirtualDom::new_with_props(AuditTrailView, AuditTrailViewProps { audit: Some(audit) });
        let html = ssr(&mut vdom);

        assert!(
            html.contains("a note worth auditing"),
            "expected the record's body: {html}"
        );
        assert!(
            html.contains("No logged reads or writes yet"),
            "expected the empty access-log message: {html}"
        );
        assert!(
            html.contains("No curator actions yet"),
            "expected the empty curation-history message: {html}"
        );
        assert!(
            html.contains("Proposed"),
            "expected the promotion history entry: {html}"
        );
    }
}
