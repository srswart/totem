//! Landscape ingestion: writing an enrolled repo's ARRIVE artifacts into the
//! graph, idempotently, with sync provenance (ADV-ARRIVE-SYNC-001,
//! docs/solution-intent.md §2.3).
//!
//! `totem-arrive-sync` parses `/arrive/` into a [`LandscapeSnapshot`]; these
//! tests build one by hand and exercise the store's write/read path directly,
//! independent of parsing — the same split `tests/embedding.rs` draws between
//! the embedder and the store that persists its output.

mod common;

use totem_store::{
    AdvanceArtifact, ComponentArtifact, LandscapeSnapshot, OwnerArtifact, RepoArtifact,
    SystemArtifact,
};

fn snapshot() -> LandscapeSnapshot {
    LandscapeSnapshot {
        repo: RepoArtifact {
            id: "058-totem".to_string(),
            name: "058 Totem".to_string(),
            git_repo: "srswart/totem".to_string(),
        },
        systems: vec![SystemArtifact {
            id: "058-totem-core".to_string(),
            name: "058 Totem Core".to_string(),
        }],
        components: vec![
            ComponentArtifact {
                id: "store".to_string(),
                system: "058-totem-core".to_string(),
                name: "Totem Store".to_string(),
                stage: Some("incubating".to_string()),
                owners: vec![OwnerArtifact {
                    id: "team:058-totem".to_string(),
                    name: "058-totem".to_string(),
                }],
            },
            ComponentArtifact {
                id: "core".to_string(),
                system: "058-totem-core".to_string(),
                name: "Totem Core".to_string(),
                stage: Some("incubating".to_string()),
                owners: vec![OwnerArtifact {
                    id: "team:058-totem".to_string(),
                    name: "058-totem".to_string(),
                }],
            },
        ],
        advances: vec![AdvanceArtifact {
            id: "ADV-STORE-001".to_string(),
            system: "058-totem-core".to_string(),
            title: "SurrealDB schema + repositories + scope-isolation tests".to_string(),
            status: Some("complete".to_string()),
            components: vec!["store".to_string(), "core".to_string()],
        }],
    }
}

#[tokio::test]
async fn a_sync_writes_the_repo_system_component_and_advance() {
    let store = common::store().await;

    let summary = store
        .landscape()
        .sync(&snapshot(), "test")
        .await
        .expect("sync succeeds");
    assert_eq!(summary.systems, 1);
    assert_eq!(summary.components, 2);
    assert_eq!(summary.advances, 1);

    let view = store
        .landscape()
        .view("058-totem")
        .await
        .expect("view succeeds");

    assert_eq!(
        view.repo.as_ref().map(|repo| repo.name.as_str()),
        Some("058 Totem")
    );
    assert_eq!(
        view.repo.as_ref().and_then(|repo| repo.git_repo.as_deref()),
        Some("srswart/totem"),
    );
    assert_eq!(view.systems.len(), 1);
    assert_eq!(view.systems[0].id, "058-totem-core");
    assert_eq!(view.systems[0].name, "058 Totem Core");

    assert_eq!(view.components.len(), 2);
    let component = view
        .components
        .iter()
        .find(|component| component.id == "store")
        .expect("the store component is in the view");
    assert_eq!(component.system, "058-totem-core");
    assert_eq!(component.name, "Totem Store");
    assert_eq!(component.stage.as_deref(), Some("incubating"));
    assert_eq!(component.owners, vec!["058-totem".to_string()]);

    assert_eq!(view.advances.len(), 1);
    let advance = &view.advances[0];
    assert_eq!(advance.id, "ADV-STORE-001");
    assert_eq!(advance.system, "058-totem-core");
    assert_eq!(advance.status.as_deref(), Some("complete"));
    let mut impacted = advance.components.clone();
    impacted.sort();
    assert_eq!(impacted, vec!["core".to_string(), "store".to_string()]);
}

#[tokio::test]
async fn repo_reads_the_same_row_view_does_without_the_full_landscape() {
    let store = common::store().await;
    let landscape = store.landscape();
    landscape
        .sync(&snapshot(), "test")
        .await
        .expect("sync succeeds");

    let repo = landscape
        .repo("058-totem")
        .await
        .expect("repo lookup succeeds")
        .expect("the repo was synced");
    assert_eq!(repo.name, "058 Totem");
    assert_eq!(repo.git_repo.as_deref(), Some("srswart/totem"));

    let unsynced = landscape
        .repo("nothing-here")
        .await
        .expect("repo lookup succeeds");
    assert!(unsynced.is_none());
}

#[tokio::test]
async fn a_view_for_an_unsynced_repo_is_empty() {
    let store = common::store().await;
    let view = store
        .landscape()
        .view("nothing-here")
        .await
        .expect("view succeeds");
    assert!(view.repo.is_none());
    assert!(view.systems.is_empty());
    assert!(view.components.is_empty());
    assert!(view.advances.is_empty());
}

#[tokio::test]
async fn re_syncing_is_idempotent_and_drops_stale_edges() {
    let store = common::store().await;
    let landscape = store.landscape();

    landscape
        .sync(&snapshot(), "test")
        .await
        .expect("first sync");

    // Second sync: the advance no longer impacts `core`, and the component
    // has a new owner. Neither the stale `impacts` edge nor the stale
    // `owned_by` edge should survive the re-sync.
    let mut second = snapshot();
    second.advances[0].components = vec!["store".to_string()];
    second.components[0].owners = vec![OwnerArtifact {
        id: "team:new-owner".to_string(),
        name: "new-owner".to_string(),
    }];
    landscape.sync(&second, "test").await.expect("second sync");

    let view = landscape.view("058-totem").await.expect("view succeeds");
    assert_eq!(view.systems.len(), 1, "re-sync must not duplicate systems");
    assert_eq!(
        view.components.len(),
        2,
        "re-sync must not duplicate components"
    );
    assert_eq!(
        view.advances.len(),
        1,
        "re-sync must not duplicate advances"
    );
    let store_component = view
        .components
        .iter()
        .find(|component| component.id == "store")
        .expect("the store component is in the view");
    assert_eq!(store_component.owners, vec!["new-owner".to_string()]);
    assert_eq!(view.advances[0].components, vec!["store".to_string()]);
}

#[tokio::test]
async fn advance_reads_one_advance_by_id_without_a_repo_qualifier() {
    let store = common::store().await;
    store
        .landscape()
        .sync(&snapshot(), "test")
        .await
        .expect("sync succeeds");

    let advance = store
        .landscape()
        .advance("ADV-STORE-001")
        .await
        .expect("advance lookup succeeds")
        .expect("the advance was synced");
    assert_eq!(advance.id, "ADV-STORE-001");
    assert_eq!(advance.system, "058-totem-core");
    assert_eq!(advance.status.as_deref(), Some("complete"));
    let mut impacted = advance.components.clone();
    impacted.sort();
    assert_eq!(impacted, vec!["core".to_string(), "store".to_string()]);
}

#[tokio::test]
async fn advance_for_an_unsynced_id_is_none_not_an_error() {
    let store = common::store().await;

    let advance = store
        .landscape()
        .advance("ADV-NEVER-SYNCED-999")
        .await
        .expect("advance lookup succeeds");
    assert!(advance.is_none());
}

#[tokio::test]
async fn every_sync_appends_a_provenance_row() {
    let store = common::store().await;
    let landscape = store.landscape();

    landscape
        .sync(&snapshot(), "dogfood:058-totem-core")
        .await
        .expect("first sync");
    landscape
        .sync(&snapshot(), "dogfood:058-totem-core")
        .await
        .expect("second sync");

    let runs = landscape.sync_runs().await.expect("sync_runs readable");
    assert_eq!(runs.len(), 2, "each run appends its own provenance row");
    assert!(
        runs.iter()
            .all(|run| run.source == "dogfood:058-totem-core")
    );
    assert!(runs.iter().all(|run| run.advances_synced == 1));
}
