//! Validated identifier newtypes.
//!
//! Scope and provenance both hang off these, so an id that is empty or
//! carries stray whitespace must be rejected at construction rather than
//! reaching the store, where `actor:ada` and `actor: ada` would name two
//! different owners of the same private memory.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

/// Why a string was refused as an identifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// The value was empty or contained only whitespace.
    #[error("{kind} identifier must not be empty")]
    Empty {
        /// The identifier kind that was being constructed.
        kind: &'static str,
    },
    /// The value carried leading or trailing whitespace.
    #[error("{kind} identifier must not have leading or trailing whitespace: {value:?}")]
    Untrimmed {
        /// The identifier kind that was being constructed.
        kind: &'static str,
        /// The value as supplied.
        value: String,
    },
}

fn validate(kind: &'static str, value: &str) -> Result<(), IdError> {
    if value.trim().is_empty() {
        return Err(IdError::Empty { kind });
    }
    if value.trim() != value {
        return Err(IdError::Untrimmed {
            kind,
            value: value.to_string(),
        });
    }
    Ok(())
}

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident, $kind:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Construct the identifier, rejecting empty or untrimmed values.
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                validate($kind, &value)?;
                Ok(Self(value))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

string_id!(
    /// A human or agent identity: the owner of `actor` scope.
    ActorId,
    "actor"
);
string_id!(
    /// An enrolled repository, named as `owner/name`.
    RepoId,
    "repo"
);
string_id!(
    /// A team that shares `team` scope.
    TeamId,
    "team"
);
string_id!(
    /// One harness session, as reported by the calling harness.
    SessionId,
    "session"
);

/// The identity of a single memory record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryId(Uuid);

impl MemoryId {
    /// Mint a fresh identifier for a new record.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MemoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MemoryId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_reject_empty_and_untrimmed_values() {
        assert_eq!(ActorId::new(""), Err(IdError::Empty { kind: "actor" }));
        assert!(matches!(
            ActorId::new("ada "),
            Err(IdError::Untrimmed { kind: "actor", .. })
        ));
    }

    #[test]
    fn deserialising_an_invalid_id_fails() {
        assert!(serde_json::from_str::<ActorId>("\"\"").is_err());
        assert!(serde_json::from_str::<ActorId>("\" ada\"").is_err());
    }

    #[test]
    fn memory_ids_are_unique_and_round_trip_as_text() {
        let id = MemoryId::new();
        assert_ne!(id, MemoryId::new());
        assert_eq!(id.to_string().parse::<MemoryId>().expect("parses"), id);
    }
}
