//! Provenance: who wrote a memory, from which harness, session, and turn.
//!
//! Every write carries this (docs/project-brief.md, G3). [`Provenance`]
//! deliberately implements neither `Default` nor a builder that starts empty:
//! there is no way to construct one without naming an author, a harness, a
//! session, and a time, so no convenience constructor can quietly produce an
//! unattributable record.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, MemoryId, SessionId};

/// Who authored a memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "actor")]
pub enum Author {
    /// A person, writing through the console or a harness.
    Human(ActorId),
    /// An agent working on a person's behalf.
    Agent(ActorId),
    /// One of Totem's own maintenance agents.
    Curator(ActorId),
}

impl Author {
    /// The identity behind the write, whichever kind of author it is.
    pub fn actor(&self) -> &ActorId {
        match self {
            Author::Human(id) | Author::Agent(id) | Author::Curator(id) => id,
        }
    }
}

/// The harness a write arrived through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Harness {
    /// Claude Code, desktop or cloud.
    ClaudeCode,
    /// Cursor, foreground or background agent.
    Cursor,
    /// A cloud agent that is neither of the above.
    CloudAgent,
    /// Totem's own web console.
    Console,
    /// An internal curator job.
    Curator,
    /// A harness Totem does not know by name yet.
    Other(String),
}

/// Where a memory came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Who wrote it.
    pub author: Author,
    /// Which harness the write arrived through.
    pub harness: Harness,
    /// The harness session the write belongs to.
    pub session: SessionId,
    /// The turn within that session, when the harness reports one.
    pub turn: Option<u32>,
    /// When the write happened.
    pub created_at: DateTime<Utc>,
    /// The records this one was derived from — typically episodic sources.
    pub derived_from: Vec<MemoryId>,
}

impl Provenance {
    /// Record a write. Every field a reader needs to answer "why did the agent
    /// believe that?" is required here.
    pub fn new(
        author: Author,
        harness: Harness,
        session: SessionId,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            author,
            harness,
            session,
            turn: None,
            created_at,
            derived_from: Vec::new(),
        }
    }

    /// Attach the turn within the session.
    pub fn at_turn(mut self, turn: u32) -> Self {
        self.turn = Some(turn);
        self
    }

    /// Link the sources this memory was derived from.
    pub fn derived_from(mut self, sources: impl IntoIterator<Item = MemoryId>) -> Self {
        self.derived_from = sources.into_iter().collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provenance() -> Provenance {
        Provenance::new(
            Author::Curator(ActorId::new("dedupe").expect("valid actor id")),
            Harness::Curator,
            SessionId::new("job-7").expect("valid session id"),
            "2026-08-05T06:00:00Z".parse().expect("valid timestamp"),
        )
    }

    #[test]
    fn every_author_kind_exposes_its_actor() {
        let id = ActorId::new("ada").expect("valid actor id");
        for author in [
            Author::Human(id.clone()),
            Author::Agent(id.clone()),
            Author::Curator(id.clone()),
        ] {
            assert_eq!(author.actor(), &id);
        }
    }

    #[test]
    fn turn_and_sources_are_optional_additions() {
        let base = provenance();
        assert_eq!(base.turn, None);
        assert!(base.derived_from.is_empty());

        let source = MemoryId::new();
        let enriched = provenance().at_turn(3).derived_from([source]);
        assert_eq!(enriched.turn, Some(3));
        assert_eq!(enriched.derived_from, vec![source]);
    }

    #[test]
    fn provenance_round_trips_through_json() {
        let original = provenance().at_turn(1).derived_from([MemoryId::new()]);
        let json = serde_json::to_string(&original).expect("serialises");
        let back: Provenance = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back, original);
    }
}
