//! DEP-001 durability: data survives a reopen, the engine lock enforces the
//! single-owner rule physically, and a directory-snapshot backup restores.
//!
//! Gated behind the `rocksdb` feature (`required-features` in Cargo.toml):
//! default builds — including every cloud/CI runner — never compile or run
//! these, per the infra component's lean-default invariant. Run them with:
//!
//! ```sh
//! cargo test -p totem-store --features rocksdb --test durability
//! ```

mod common;

use std::fs;
use std::path::Path;

use common::{ADA, chain, memory};
use surrealdb::engine::local::Db;
use totem_core::{MemoryCategory, Scope};
use totem_store::Store;

/// Open the on-disk store, retrying briefly while a just-dropped instance
/// finishes releasing the engine lock.
///
/// Dropping a `Store` closes the engine *asynchronously*: the SDK's router
/// task calls `kvs.shutdown()` only after the last handle's channel closes,
/// so the RocksDB `LOCK` file frees a moment after `drop` returns. A real
/// gateway restart is a process exit and never sees this; same-process
/// reopens (these tests) must wait it out. The bound keeps a genuine
/// second-owner conflict from hanging: `the_engine_lock_refuses_a_second_owner`
/// calls `Store::on_disk` directly, without this retry.
async fn open(dir: &Path) -> Store<Db> {
    let mut last_err = None;
    for _ in 0..50 {
        match Store::on_disk(dir).await {
            Ok(store) => {
                store.migrate().await.expect("migrations apply");
                return store;
            }
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
    panic!("on-disk engine never opened: {last_err:?}");
}

/// Copy a directory tree — the backup "tool" under test is deliberately
/// nothing more than a recursive file copy of a closed data directory, per
/// the offline-snapshot decision recorded in the advance.
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create target dir");
    for entry in fs::read_dir(from).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

#[tokio::test]
async fn memories_survive_a_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let record = memory(
        MemoryCategory::Knowledge,
        Scope::Project(common::repo()),
        "this memory must survive a gateway restart",
    );
    let id = record.id;

    {
        let store = open(dir.path()).await;
        store
            .memories()
            .save(&chain(ADA), &record)
            .await
            .expect("write");
    }

    let reopened = open(dir.path()).await;
    let stored = reopened
        .memories()
        .get(&chain(ADA), id)
        .await
        .expect("get succeeds")
        .expect("the memory survived the reopen");
    assert_eq!(
        stored.content.body,
        "this memory must survive a gateway restart"
    );

    let versions = reopened.applied_migrations().await.expect("ledger reads");
    assert!(
        !versions.is_empty(),
        "the migration ledger itself is durable"
    );
}

#[tokio::test]
async fn the_engine_lock_refuses_a_second_owner() {
    let dir = tempfile::tempdir().expect("tempdir");
    let _first = open(dir.path()).await;

    let second = Store::on_disk(dir.path()).await;
    assert!(
        second.is_err(),
        "a second store on the same data directory must fail on the engine \
         lock (DEP-001's single-owner rule is physical), got Ok"
    );
}

#[tokio::test]
async fn a_directory_snapshot_backup_restores() {
    let data = tempfile::tempdir().expect("data dir");
    let backup = tempfile::tempdir().expect("backup dir");
    let record = memory(
        MemoryCategory::Context,
        Scope::Actor(common::actor(ADA)),
        "the backup must carry this record",
    );
    let id = record.id;

    // Write, then close the store — offline snapshots copy a closed dir.
    {
        let store = open(data.path()).await;
        store
            .memories()
            .save(&chain(ADA), &record)
            .await
            .expect("write");
    }

    // Backup, destroy, restore.
    let snapshot = backup.path().join("snapshot");
    copy_dir(data.path(), &snapshot);
    fs::remove_dir_all(data.path()).expect("wipe the data dir");
    copy_dir(&snapshot, data.path());

    // The restored directory is a fully functional store with the record.
    let restored = open(data.path()).await;
    let stored = restored
        .memories()
        .get(&chain(ADA), id)
        .await
        .expect("get succeeds")
        .expect("the record came back from the snapshot");
    assert_eq!(stored.content.body, "the backup must carry this record");
}
