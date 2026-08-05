//! Scope: the isolation boundary, and the chain a reader is allowed to see.
//!
//! Leaking private context across scopes is the project's highest-severity
//! failure (docs/project-brief.md, "Key risks"), so the readable set is an
//! explicit, constructed value rather than a filter applied downstream. A
//! caller cannot widen its reach by forgetting a `WHERE` clause: it can only
//! read what its [`ScopeChain`] already lists.
//!
//! This module defines the boundary. Enforcing it on every read and write path
//! is `totem-store`'s job (ADV-STORE-001).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::{ActorId, IdError, RepoId, TeamId};

/// The scope a memory belongs to.
///
/// Serialises as its wire form — `actor:ada`, `project:srswart/totem`,
/// `team:058-totem`, `platform` — so the same text appears in the store, the
/// API, and the audit log.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    /// Private to one developer or one agent identity.
    Actor(ActorId),
    /// Shared by everyone working an enrolled repo.
    Project(RepoId),
    /// Cross-project team conventions.
    Team(TeamId),
    /// The shared landscape every enrolled actor sees.
    Platform,
}

/// Why a string was not a valid scope.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScopeParseError {
    /// The prefix was not one of `actor`, `project`, `team`, or `platform`.
    #[error("unknown scope prefix: {0:?}")]
    UnknownPrefix(String),
    /// `platform` is unqualified but an identifier was supplied.
    #[error("the platform scope takes no identifier")]
    UnexpectedId,
    /// The identifier after the prefix was not valid.
    #[error(transparent)]
    Id(#[from] IdError),
}

impl Scope {
    /// How specific this scope is: `0` is the narrowest (`actor`), `3` the
    /// widest (`platform`). Used to order a resolved chain.
    pub fn specificity(&self) -> u8 {
        match self {
            Scope::Actor(_) => 0,
            Scope::Project(_) => 1,
            Scope::Team(_) => 2,
            Scope::Platform => 3,
        }
    }

    /// Whether this scope is private to a single actor.
    pub fn is_private(&self) -> bool {
        matches!(self, Scope::Actor(_))
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scope::Actor(id) => write!(f, "actor:{id}"),
            Scope::Project(id) => write!(f, "project:{id}"),
            Scope::Team(id) => write!(f, "team:{id}"),
            Scope::Platform => f.write_str("platform"),
        }
    }
}

impl FromStr for Scope {
    type Err = ScopeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.split_once(':') {
            Some(("actor", id)) => Ok(Scope::Actor(ActorId::new(id)?)),
            Some(("project", id)) => Ok(Scope::Project(RepoId::new(id)?)),
            Some(("team", id)) => Ok(Scope::Team(TeamId::new(id)?)),
            Some(("platform", _)) => Err(ScopeParseError::UnexpectedId),
            Some((prefix, _)) => Err(ScopeParseError::UnknownPrefix(prefix.to_string())),
            None if value == "platform" => Ok(Scope::Platform),
            None => Err(ScopeParseError::UnknownPrefix(value.to_string())),
        }
    }
}

impl Serialize for Scope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// The ordered set of scopes one caller may read, narrowest first.
///
/// Built only by [`ScopeChain::resolve`] from a caller's own identity and
/// memberships, so it can never name another actor's private scope. Position in
/// the chain is precedence: when the same fact exists at several scopes, the
/// earlier one wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeChain {
    scopes: Vec<Scope>,
}

impl ScopeChain {
    /// Resolve the chain for one actor: their own private scope, the project
    /// they are working in (if any), each team they belong to, then platform.
    ///
    /// The actor's own id is the only `actor` scope that can enter the chain.
    pub fn resolve(actor: &ActorId, project: Option<&RepoId>, teams: &[TeamId]) -> Self {
        let mut scopes = Vec::with_capacity(teams.len() + 3);
        scopes.push(Scope::Actor(actor.clone()));
        if let Some(project) = project {
            scopes.push(Scope::Project(project.clone()));
        }
        for team in teams {
            let scope = Scope::Team(team.clone());
            if !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }
        scopes.push(Scope::Platform);
        Self { scopes }
    }

    /// The chain, narrowest scope first.
    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }

    /// Whether this chain permits reading the given scope.
    pub fn contains(&self, scope: &Scope) -> bool {
        self.scopes.contains(scope)
    }

    /// The scope's precedence, lower being more specific, or `None` when the
    /// chain does not permit it.
    pub fn precedence_of(&self, scope: &Scope) -> Option<usize> {
        self.scopes.iter().position(|candidate| candidate == scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(id: &str) -> ActorId {
        ActorId::new(id).expect("valid actor id")
    }

    #[test]
    fn specificity_orders_narrow_before_wide() {
        assert!(Scope::Actor(actor("ada")).specificity() < Scope::Platform.specificity());
        assert!(Scope::Actor(actor("ada")).is_private());
        assert!(!Scope::Platform.is_private());
    }

    #[test]
    fn a_chain_is_ordered_by_specificity() {
        let chain = ScopeChain::resolve(
            &actor("ada"),
            Some(&RepoId::new("srswart/totem").expect("valid repo id")),
            &[TeamId::new("058-totem").expect("valid team id")],
        );

        let specificities: Vec<u8> = chain.scopes().iter().map(Scope::specificity).collect();
        assert!(specificities.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_chain_holds_exactly_one_private_scope() {
        let chain = ScopeChain::resolve(&actor("ada"), None, &[]);
        let private: Vec<&Scope> = chain.scopes().iter().filter(|s| s.is_private()).collect();
        assert_eq!(private, vec![&Scope::Actor(actor("ada"))]);
    }
}
