//! Fixtures shared by the store's integration tests.
//!
//! Every test builds its own embedded `kv-mem` instance and seeds its own data
//! (docs/tech-direction/surrealdb.md §4): nothing here assumes a running
//! `surreal` server, a port, or a container.

use chrono::{DateTime, Utc};
use surrealdb::engine::local::Db;
use totem_core::{
    ActorId, Author, Content, Harness, MemoryCategory, MemoryRecord, Provenance, RepoId, Scope,
    ScopeChain, SessionId, TeamId,
};
use totem_store::{EMBEDDING_DIMENSIONS, Store};

pub const ADA: &str = "ada";
pub const GRACE: &str = "grace";
pub const REPO: &str = "srswart/totem";
pub const TEAM: &str = "058-totem";

/// A migrated, empty store.
pub async fn store() -> Store<Db> {
    let store = Store::in_memory().await.expect("embedded engine connects");
    store.migrate().await.expect("migrations apply");
    store
}

pub fn actor(id: &str) -> ActorId {
    ActorId::new(id).expect("valid actor id")
}

pub fn repo() -> RepoId {
    RepoId::new(REPO).expect("valid repo id")
}

pub fn team() -> TeamId {
    TeamId::new(TEAM).expect("valid team id")
}

/// The chain for an actor working the shared repo, with no team memberships.
pub fn chain(id: &str) -> ScopeChain {
    ScopeChain::resolve(&actor(id), Some(&repo()), &[])
}

/// The chain for an actor who is also a team member.
pub fn chain_with_team(id: &str) -> ScopeChain {
    ScopeChain::resolve(&actor(id), Some(&repo()), &[team()])
}

pub fn at(timestamp: &str) -> DateTime<Utc> {
    timestamp.parse().expect("valid RFC 3339 timestamp")
}

/// A record with the minimum every write needs: category, scope, body, and
/// provenance naming an author, harness, session, and time.
pub fn memory(category: MemoryCategory, scope: Scope, body: &str) -> MemoryRecord {
    written_at(category, scope, body, "2026-08-05T06:00:00Z")
}

pub fn written_at(
    category: MemoryCategory,
    scope: Scope,
    body: &str,
    timestamp: &str,
) -> MemoryRecord {
    MemoryRecord::new(
        category,
        scope,
        Content::new(body),
        Provenance::new(
            Author::Agent(actor(ADA)),
            Harness::ClaudeCode,
            SessionId::new("sess-1").expect("valid session id"),
            at(timestamp),
        ),
    )
}

/// A unit vector of the pinned dimension, pointing along axis `axis`.
///
/// Distinct axes are maximally far apart under cosine distance, which is what
/// makes "the nearest readable row" unambiguous in the recall tests.
pub fn unit_vector(axis: usize) -> Vec<f32> {
    let mut embedding = vec![0.0; EMBEDDING_DIMENSIONS];
    embedding[axis % EMBEDDING_DIMENSIONS] = 1.0;
    embedding
}

pub fn bodies(records: &[MemoryRecord]) -> Vec<String> {
    records
        .iter()
        .map(|record| record.content.body.clone())
        .collect()
}

pub fn sorted_bodies(records: &[MemoryRecord]) -> Vec<String> {
    let mut bodies = bodies(records);
    bodies.sort();
    bodies
}
