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
use totem_core::MemoryRecord;

use crate::view_model::{AdvanceView, ComponentView, LandscapeViewModel, group_by_category};

/// Which tab the console is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// The landscape dashboard.
    Landscape,
    /// The memory browser.
    Memories,
}

/// Root component: a tab switcher over the two read-only views.
#[component]
pub fn App(landscape: LandscapeViewModel, memories: Vec<MemoryRecord>) -> Element {
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
            }
            main {
                if *tab.read() == Tab::Landscape {
                    LandscapeView { view: landscape.clone() }
                } else {
                    MemoryBrowserView { records: memories.clone() }
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

    #[test]
    fn the_app_shell_starts_on_the_landscape_tab() {
        let mut vdom = VirtualDom::new_with_props(
            App,
            AppProps {
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
    }
}
