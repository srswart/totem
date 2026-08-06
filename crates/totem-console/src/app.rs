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

    rsx! {
        div { class: "totem-console",
            nav { class: "totem-console__tabs",
                button {
                    class: if *tab.read() == Tab::Landscape { "active" } else { "" },
                    onclick: move |_| tab.set(Tab::Landscape),
                    "Landscape"
                }
                button {
                    class: if *tab.read() == Tab::Memories { "active" } else { "" },
                    onclick: move |_| tab.set(Tab::Memories),
                    "Memories"
                }
                button {
                    class: if *tab.read() == Tab::Governance { "active" } else { "" },
                    onclick: move |_| tab.set(Tab::Governance),
                    "Governance"
                }
                button {
                    class: if *tab.read() == Tab::Audit { "active" } else { "" },
                    onclick: move |_| tab.set(Tab::Audit),
                    "Audit"
                }
            }
            main {
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
        section { class: "promotion-queue",
            h3 { "Promotions awaiting approval" }
            if promotions.is_empty() {
                p { class: "empty", "No promotions are waiting on a decision." }
            }
            ul {
                for proposal in promotions.iter() {
                    li { key: "{proposal.id}",
                        span { class: "promotion-move",
                            "{proposal.memory}: {proposal.from_scope} \u{2192} {proposal.to_scope}"
                        }
                        {
                            let id = proposal.id;
                            rsx! {
                                button { onclick: move |_| on_approve.call(id), "Approve" }
                                button { onclick: move |_| on_reject.call(id), "Reject" }
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
        section { class: "uncertainty-queue",
            h3 { "Uncertainty awaiting resolution" }
            if records.is_empty() {
                p { class: "empty", "No contested memories are waiting on a decision." }
            }
            ul {
                for record in records.iter() {
                    li { key: "{record.id}",
                        span { class: "scope", "{record.scope}" }
                        " — {record.content.body}"
                        {
                            let id = record.id;
                            rsx! {
                                button {
                                    onclick: move |_| on_resolve.call((id, ReviewState::Approved)),
                                    "Approve"
                                }
                                button {
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
                p { class: "empty", "Look up a memory id to see its audit trail." }
            }
        };
    };

    rsx! {
        section { class: "audit-trail",
            h3 { "Record" }
            p { "{audit.record.content.body}" }
            p { class: "provenance",
                "written by {audit.record.provenance.author.actor()} via {audit.record.provenance.harness:?} "
                "at {audit.record.provenance.created_at}"
            }
            h3 { "Access history" }
            if audit.access_log.is_empty() {
                p { class: "empty", "No logged reads or writes yet." }
            }
            ul {
                for entry in audit.access_log.iter() {
                    li { key: "{entry.at}", "{entry.operation:?} via {entry.endpoint} at {entry.at}" }
                }
            }
            h3 { "Curator lineage" }
            if audit.curation_history.is_empty() {
                p { class: "empty", "No curator actions yet." }
            }
            ul {
                for event in audit.curation_history.iter() {
                    li { key: "{event.id}", "{event.kind:?} at {event.provenance.created_at}" }
                }
            }
            h3 { "Promotion history" }
            if audit.promotion_history.is_empty() {
                p { class: "empty", "This record has never changed scope." }
            }
            ul {
                for event in audit.promotion_history.iter() {
                    li { key: "{event.id}",
                        "{event.kind:?}: {event.from_scope} \u{2192} {event.to_scope}"
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
        section { class: "landscape",
            if let Some(repo) = &view.repo {
                h2 { "{repo.name}" }
            } else {
                p { class: "empty", "This repo has not been synced yet." }
            }
            h3 { "Systems" }
            ul {
                for system in view.systems.iter() {
                    li { key: "{system.id}", "{system.name}" }
                }
            }
            h3 { "Components" }
            ul {
                for component in view.components.iter() {
                    ComponentRow { component: component.clone() }
                }
            }
            h3 { "Advances" }
            ul {
                for advance in view.advances.iter() {
                    AdvanceRow { advance: advance.clone() }
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
        li { key: "{component.id}",
            strong { "{component.id}" }
            " — {component.name} "
            span { class: "stage", "[{stage}]" }
            if !owners.is_empty() {
                span { class: "owners", " (owners: {owners})" }
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
        li { key: "{advance.id}",
            strong { "{advance.id}" }
            " — {advance.title} "
            span { class: "status", "[{status}]" }
            if !components.is_empty() {
                span { class: "components", " (impacts: {components})" }
            }
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
        section { class: "memory-browser",
            if records.is_empty() {
                p { class: "empty", "No memories in view for this scope chain." }
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
            h3 { "{label}" }
            ul {
                for record in records.iter() {
                    li { key: "{record.id}",
                        span { class: "scope", "{record.scope}" }
                        " — {record.content.body}"
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
        assert!(html.contains("Approve"), "expected an Approve button: {html}");
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
        assert!(html.contains("Approve"), "expected an Approve button: {html}");
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
        let mut vdom = VirtualDom::new_with_props(AuditTrailView, AuditTrailViewProps { audit: None });
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
        let mut vdom = VirtualDom::new_with_props(
            AuditTrailView,
            AuditTrailViewProps { audit: Some(audit) },
        );
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
        assert!(html.contains("Proposed"), "expected the promotion history entry: {html}");
    }
}
