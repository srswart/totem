//! Totem's domain model: typed agent memory, the scopes that isolate it, and
//! the provenance that makes it auditable.
//!
//! This crate is types and rules only — no storage, transport, or ranking
//! behaviour. `totem-store` persists these types and enforces scope isolation
//! and append-only episodic history at the persistence layer (ADV-STORE-001);
//! `totem-gateway` exposes them over MCP and REST.
//!
//! Three rules are load-bearing and are established here so that everything
//! built on top inherits them:
//!
//! - A memory is **exactly one** of six [`MemoryCategory`] values, and the
//!   category — not the caller — decides mutability, TTL, decay, review, and
//!   injection priority.
//! - Every memory carries a [`Scope`], and a reader sees only the scopes in its
//!   own resolved [`ScopeChain`].
//! - Every memory carries [`Provenance`]; there is no way to build one without
//!   an author, harness, session, and timestamp.
//!
//! ```
//! use chrono::Utc;
//! use totem_core::{
//!     ActorId, Author, Content, Harness, MemoryCategory, MemoryRecord, Provenance, RepoId,
//!     Scope, ScopeChain, SessionId,
//! };
//!
//! let ada = ActorId::new("ada")?;
//! let repo = RepoId::new("srswart/totem")?;
//!
//! let note = MemoryRecord::new(
//!     MemoryCategory::Knowledge,
//!     Scope::Project(repo.clone()),
//!     Content::new("scope isolation is enforced in totem-store"),
//!     Provenance::new(
//!         Author::Agent(ada.clone()),
//!         Harness::ClaudeCode,
//!         SessionId::new("sess-1")?,
//!         Utc::now(),
//!     ),
//! );
//!
//! let chain = ScopeChain::resolve(&ada, Some(&repo), &[]);
//! assert!(chain.contains(&note.scope));
//! assert!(!chain.contains(&Scope::Actor(ActorId::new("grace")?)));
//! # Ok::<(), totem_core::IdError>(())
//! ```

#![warn(missing_docs)]

mod access_log;
mod category;
mod ids;
mod provenance;
mod record;
mod scope;

pub use access_log::{AccessLogEntry, AccessOperation};
pub use category::{CategoryLifecycle, MemoryCategory, Mutability, ReviewPolicy};
pub use ids::{ActorId, IdError, MemoryId, RepoId, SessionId, TeamId};
pub use provenance::{Author, Harness, Provenance};
pub use record::{
    Content, Economics, Governance, LifecycleError, MemoryRecord, MemoryStatus, ReviewState,
    SubjectKind, SubjectRef,
};
pub use scope::{Scope, ScopeChain, ScopeParseError};
