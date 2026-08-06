//! Ingest this repo's own `/arrive/` tree — the dogfood ADV-ARRIVE-SYNC-001
//! is named for. Real fixtures: whatever `/arrive/` actually contains, not a
//! synthetic stand-in, so a schema-drift bug between the artifact writer
//! (`arrive` CLI) and this reader shows up here first.

use std::path::{Path, PathBuf};

use surrealdb::engine::local::Db;
use totem_store::Store;

fn arrive_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../arrive")
}

async fn migrated_store() -> Store<Db> {
    let store = Store::in_memory().await.expect("embedded engine connects");
    store.migrate().await.expect("migrations apply");
    store
}

#[test]
fn parsing_this_repos_arrive_tree_finds_the_expected_entities() {
    let snapshot = totem_arrive_sync::read_repo_artifacts(&arrive_root())
        .expect("this repo's own /arrive/ parses");

    assert_eq!(snapshot.repo.id, "058-totem");
    assert_eq!(snapshot.repo.name, "058 Totem");

    assert_eq!(snapshot.systems.len(), 1);
    assert_eq!(snapshot.systems[0].id, "058-totem-core");

    let component_ids: Vec<&str> = {
        let mut ids: Vec<&str> = snapshot
            .components
            .iter()
            .map(|component| component.id.as_str())
            .collect();
        ids.sort_unstable();
        ids
    };
    assert_eq!(
        component_ids,
        vec![
            "arrive-sync",
            "cli",
            "console",
            "core",
            "curator",
            "gateway",
            "store",
        ],
    );
    let store_component = snapshot
        .components
        .iter()
        .find(|component| component.id == "store")
        .expect("the store component parsed");
    assert_eq!(store_component.stage.as_deref(), Some("incubating"));
    assert_eq!(
        store_component.owners,
        vec![totem_store::OwnerArtifact {
            id: "team:058-totem".to_string(),
            name: "058-totem".to_string(),
        }],
    );

    assert!(
        snapshot.advances.len() >= 23,
        "expected at least the 23 advances documented in cloud-agent-notes.md, got {}",
        snapshot.advances.len(),
    );
    let store_001 = snapshot
        .advances
        .iter()
        .find(|advance| advance.id == "ADV-STORE-001")
        .expect("ADV-STORE-001 parsed");
    assert_eq!(store_001.status.as_deref(), Some("complete"));
    let mut components = store_001.components.clone();
    components.sort();
    assert_eq!(components, vec!["core".to_string(), "store".to_string()]);
}

#[tokio::test]
async fn syncing_this_repos_landscape_populates_the_store_and_is_queryable_in_one_round_trip() {
    let store = migrated_store().await;

    let summary = totem_arrive_sync::sync_repo(&store, &arrive_root(), "dogfood:058-totem-core")
        .await
        .expect("sync succeeds");
    assert_eq!(summary.systems, 1);
    assert_eq!(summary.components, 7);
    assert!(summary.advances >= 23);

    let view = store
        .landscape()
        .view("058-totem")
        .await
        .expect("view succeeds");
    assert_eq!(
        view.repo.map(|repo| repo.name),
        Some("058 Totem".to_string())
    );
    assert_eq!(view.systems.len(), 1);
    assert_eq!(view.components.len(), 7);
    assert_eq!(view.advances.len(), summary.advances);

    let store_component = view
        .components
        .iter()
        .find(|component| component.id == "store")
        .expect("the store component is in the view");
    assert_eq!(store_component.owners, vec!["058-totem".to_string()]);

    let sync_001 = view
        .advances
        .iter()
        .find(|advance| advance.id == "ADV-ARRIVE-SYNC-001")
        .expect("this advance's own record is in the view");
    assert!(sync_001.components.contains(&"arrive-sync".to_string()));
    assert!(sync_001.components.contains(&"store".to_string()));
}

#[test]
fn a_missing_arrive_directory_is_reported_plainly() {
    let error =
        totem_arrive_sync::read_repo_artifacts(Path::new("/nonexistent/totem-arrive-sync-fixture"))
            .expect_err("a missing directory cannot parse");
    assert!(matches!(error, totem_arrive_sync::IngestError::Io { .. }));
}
