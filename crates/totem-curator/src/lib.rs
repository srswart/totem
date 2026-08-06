//! Totem's curation jobs: background agents that tidy memory through the same
//! scope-resolved API every other caller uses (docs/solution-intent.md §5).
//!
//! A curator is not privileged. It holds no database connection of its own, it
//! is given a [`ScopeChain`] like anyone else, and every refusal the store
//! makes for a human it makes for a curator: it cannot read outside its chain,
//! cannot write outside it, and cannot delete anything at all. What it *can*
//! do is supersede — write a record that replaces others and retire them —
//! and that action is recorded as an event which a rollback can undo.
//!
//! The runner ([`Curator`]) owns the two things a job must not be trusted to
//! remember: the curator's own identity on every write, and an access log entry
//! for every scan, merge, and rollback. The job ([`dedupe`](Curator::dedupe))
//! owns only the question of which records are duplicates of each other.
//!
//! ```no_run
//! use totem_core::{ActorId, RepoId, ScopeChain, SessionId};
//! use totem_curator::{Curator, DedupePolicy};
//! use totem_store::Store;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let store = Store::in_memory().await?;
//! store.migrate().await?;
//!
//! let ada = ActorId::new("ada")?;
//! let repo = RepoId::new("srswart/totem")?;
//! let chain = ScopeChain::resolve(&ada, Some(&repo), &[]);
//!
//! let curator = Curator::new(
//!     &store,
//!     ActorId::new("totem-curator")?,
//!     SessionId::new("curate-1")?,
//! );
//! let report = curator.dedupe(&chain, &DedupePolicy::new()).await?;
//! for merge in &report.merges {
//!     // Everything it did is undoable, by the event it recorded.
//!     curator.rollback(&chain, merge.id, Some("on second thoughts".into())).await?;
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod dedupe;

use chrono::Utc;
use surrealdb::Connection;
use totem_core::{
    AccessLogEntry, AccessOperation, ActorId, Author, CurationEvent, CurationId, Harness, MemoryId,
    Provenance, ScopeChain, SessionId,
};
use totem_store::{Store, StoreResult};

pub use dedupe::{DedupePolicy, DedupeReport};

/// The endpoints a curator's access log entries carry.
///
/// The access log's `operation` vocabulary (`recall`/`save`/`feedback`) is the
/// gateway's, and a curator's scan and merge really are a read and a write —
/// so they are logged as such, and the endpoint is what distinguishes a
/// curator's read from an agent's. A dedicated `curate` operation would be a
/// core change (a new `AccessOperation` variant plus a migration), which
/// belongs to the advance that needs to *query* curator activity, not to this
/// one.
const SCAN_ENDPOINT: &str = "/curator/dedupe/scan";
const MERGE_ENDPOINT: &str = "/curator/dedupe/merge";
const ROLLBACK_ENDPOINT: &str = "/curator/dedupe/rollback";

/// A curation job runner: one curator identity, one session, one store.
#[derive(Debug)]
pub struct Curator<'a, C: Connection> {
    store: &'a Store<C>,
    actor: ActorId,
    session: SessionId,
}

impl<'a, C: Connection> Curator<'a, C> {
    /// A curator that writes as `actor`, in `session`.
    ///
    /// The identity is a parameter rather than a constant because a curator is
    /// an actor like any other: its writes are attributable to it, and its
    /// reads are logged against it.
    pub fn new(store: &'a Store<C>, actor: ActorId, session: SessionId) -> Self {
        Self {
            store,
            actor,
            session,
        }
    }

    /// Undo one merge, restoring the records it superseded.
    ///
    /// The reversibility half of the curator invariant: whatever a job did,
    /// this puts back. Refused for an event this chain cannot see, for a merge
    /// already rolled back, and for anything that is not a merge.
    pub async fn rollback(
        &self,
        chain: &ScopeChain,
        merge: CurationId,
        reason: Option<String>,
    ) -> StoreResult<CurationEvent> {
        let event = self
            .store
            .curation()
            .rollback(chain, merge, self.provenance(Vec::new()), reason)
            .await?;
        self.log(AccessOperation::Save, ROLLBACK_ENDPOINT, |entry| {
            entry.for_memory(event.merged)
        })
        .await?;
        Ok(event)
    }

    /// Provenance for a curator write: authored by the curator, through the
    /// curator harness, citing whatever it was derived from.
    fn provenance(&self, derived_from: Vec<MemoryId>) -> Provenance {
        let mut provenance = Provenance::new(
            Author::Curator(self.actor.clone()),
            Harness::Curator,
            self.session.clone(),
            Utc::now(),
        );
        provenance.derived_from = derived_from;
        provenance
    }

    /// Append one access log entry for something this curator just did.
    ///
    /// Every path that touches memory goes through here. A curator that could
    /// read or write without an entry would be an audit gap precisely where the
    /// system is least observed — nobody is watching a background job.
    async fn log(
        &self,
        operation: AccessOperation,
        endpoint: &str,
        detail: impl FnOnce(AccessLogEntry) -> AccessLogEntry,
    ) -> StoreResult<()> {
        let entry = detail(AccessLogEntry::new(
            self.actor.clone(),
            Harness::Curator,
            self.session.clone(),
            operation,
            endpoint,
            Utc::now(),
        ));
        self.store.access_log().record(&entry).await
    }
}
