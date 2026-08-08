//! The calibration corpus as a versioned artifact (ADV-STORE-009).
//!
//! [`crate::corpus`] holds a synthetic estate compiled into this binary. That
//! served ADV-STORE-005 well and cannot serve calibration, for four reasons:
//! it cannot be versioned independently of the code under test, so "recall
//! quality improved" is unfalsifiable; it cannot grow past what belongs in a
//! binary; it cannot change without a rebuild, which this project measures in
//! tens of minutes (ADV-INFRA-006/008); and a variant needs a code branch
//! rather than a different file.
//!
//! So the corpus becomes **data**: one JSON document carrying a manifest, the
//! records, and the golden queries together.
//!
//! # Two properties that are not negotiable
//!
//! **Records carry their economics.** ADV-CORE-008 proved that a corpus whose
//! records all have pristine, identical economics *cannot fail*: the three
//! non-relevance terms are then constant across the estate, they cancel, and
//! `eval_quality` scored a perfect 1.0 against a ranker that demonstrably
//! ignored the query. A corpus that cannot fail is not a test.
//!
//! **The golden queries live in the same artifact as the records.** They are
//! currently adjacent in one Rust module and would drift apart the moment
//! they were not. Questions and answers version together, or the version
//! number means nothing.
//!
//! # The checksum
//!
//! A corpus loaded from disk can be edited until an evaluation passes. The
//! manifest carries a checksum over the records and queries, so a result
//! recorded against `calibration-v1` names a specific artifact rather than
//! "whatever was in that file at the time". [`Corpus::verify`] refuses a
//! mismatch loudly rather than warning: a silently-tolerated mismatch is the
//! same as no checksum.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use totem_core::{Economics, MemoryCategory};

use crate::error::{StoreError, StoreResult};

/// What a corpus artifact is, and which one it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusManifest {
    /// Stable identifier, e.g. `calibration`.
    pub id: String,
    /// The version, e.g. `v1`. Quote this in evidence — a measurement against
    /// an unrecorded corpus version is not evidence.
    pub version: String,
    /// What this corpus is for, and what it is designed to discriminate.
    pub description: String,
    /// SHA-256 over the canonical serialization of `records` and `queries`.
    /// See [`Corpus::compute_checksum`].
    pub checksum: String,
}

/// One record in the corpus, with the economics that make ranking testable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorpusRecord {
    /// A stable handle used by golden queries to name an expected answer,
    /// so a query refers to a record rather than repeating its body — which
    /// is how the two drift apart.
    pub key: String,
    /// The category this record is written as.
    pub category: MemoryCategory,
    /// Where it is written, e.g. `actor:corpus-nova`, `project:corpus/rocket`.
    pub scope: String,
    /// The actor whose chain writes it.
    pub writer_actor: String,
    /// The project in the writer's chain, if any.
    #[serde(default)]
    pub writer_project: Option<String>,
    /// The teams in the writer's chain, if any.
    #[serde(default)]
    pub writer_teams: Vec<String>,
    /// The memory's text.
    pub body: String,
    /// Free-form tags. `GENERATOR_TAG` is added on seed, not stored here.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Which topic cluster this belongs to, if any. Clusters are what make a
    /// corpus discriminating: a query aimed at one member must prefer it over
    /// its near-misses.
    #[serde(default)]
    pub cluster: Option<String>,
    /// RFC 3339. Drives currency decay when `last_used_at` is absent.
    pub created_at: String,
    /// **The half of the record ADV-CORE-008 proved a corpus cannot omit.**
    #[serde(default = "fresh_economics")]
    pub economics: EconomicsSpec,
}

fn default_query_limit() -> usize {
    5
}

fn fresh_economics() -> EconomicsSpec {
    EconomicsSpec {
        use_count: 0,
        last_used_at: None,
        value_score: 1.0,
        currency: 1.0,
    }
}

/// A record's history, as data rather than as a default.
///
/// Separate from [`totem_core::Economics`] because that type is the runtime
/// shape and this one is a *file format*: it uses an RFC 3339 string for the
/// instant, and it must stay stable across changes to the domain type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EconomicsSpec {
    /// How many times this record has been retrieved.
    #[serde(default)]
    pub use_count: u64,
    /// When it was last retrieved, RFC 3339. `None` means never.
    #[serde(default)]
    pub last_used_at: Option<String>,
    /// How much it has earned its keep. 1.0 is neutral.
    pub value_score: f32,
    /// Freshness at the moment it was last used.
    pub currency: f32,
}

/// One recall scenario with a known-correct answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusQuery {
    /// A short label.
    pub name: String,
    /// What this query is testing, in a sentence. A golden query whose point
    /// is not written down becomes untouchable the moment it fails: nobody
    /// can tell a regression from a fixture that was always wrong.
    pub rationale: String,
    /// The reader's actor id.
    pub reader_actor: String,
    /// The project in the reader's chain, if any.
    #[serde(default)]
    pub reader_project: Option<String>,
    /// The teams in the reader's chain.
    #[serde(default)]
    pub reader_teams: Vec<String>,
    /// Restrict to these categories; empty means all.
    #[serde(default)]
    pub categories: Vec<MemoryCategory>,
    /// The text embedded as the vector probe. `None` skips vector ranking.
    #[serde(default)]
    pub probe: Option<String>,
    /// The record key expected to rank first.
    #[serde(default)]
    pub expect_top: Option<String>,
    /// Record keys that must appear anywhere in the result.
    #[serde(default)]
    pub expect_present: Vec<String>,
    /// Record keys that must **not** appear — the assertion a corpus of
    /// near-misses exists to support, and the one a top-1 check cannot make.
    ///
    /// Only meaningful together with [`Self::limit`]: recall returns its
    /// limit's worth of rows whether or not they are any good, so "absent"
    /// without a limit asserts nothing.
    #[serde(default)]
    pub expect_absent: Vec<String>,
    /// How many rows this query asks for — *what an agent would actually
    /// receive*, not how many the index will rank.
    ///
    /// Added in ADV-CORE-009. Without it every query returned ten rows out of
    /// a thirty-record corpus, so a record scoring 0.012 against a winner's
    /// 0.730 still counted as "present" and `expect_absent` could never pass.
    /// The question the corpus is asking is about the context window, and a
    /// context window has a size.
    #[serde(default = "default_query_limit")]
    pub limit: usize,
}

/// A whole corpus artifact: manifest, records, and the queries that score it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Corpus {
    /// Which corpus this is.
    pub manifest: CorpusManifest,
    /// The estate.
    pub records: Vec<CorpusRecord>,
    /// The scenarios scored against it.
    pub queries: Vec<CorpusQuery>,
}

impl Corpus {
    /// Read and **verify** an artifact.
    ///
    /// Verification is not optional here: an unverified corpus is one an
    /// evaluation could have been tuned against.
    pub fn load(path: impl AsRef<Path>) -> StoreResult<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|error| {
            StoreError::Corpus(format!("cannot read corpus at {}: {error}", path.display()))
        })?;
        let corpus: Self = serde_json::from_str(&text).map_err(|error| {
            StoreError::Corpus(format!(
                "corpus at {} is not valid: {error}",
                path.display()
            ))
        })?;
        corpus.verify()?;
        Ok(corpus)
    }

    /// The checksum this corpus's contents hash to.
    ///
    /// Over `records` and `queries` only — never the manifest, which contains
    /// the checksum itself and could not otherwise be stable. Serialized
    /// through `serde_json` rather than hashed field-by-field so the digest
    /// covers exactly what a reader of the file sees.
    pub fn compute_checksum(&self) -> String {
        let canonical = serde_json::json!({
            "records": self.records,
            "queries": self.queries,
        });
        let mut hasher = Sha256::new();
        hasher.update(canonical.to_string().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Refuse a corpus whose contents do not match its manifest.
    ///
    /// Loud, not a warning: a mismatch means the file was edited after the
    /// version was stamped, and every result quoting that version is now
    /// describing something else.
    pub fn verify(&self) -> StoreResult<()> {
        let actual = self.compute_checksum();
        if actual != self.manifest.checksum {
            return Err(StoreError::Corpus(format!(
                "corpus {}-{} does not match its checksum: manifest says {}, contents hash to {}. \
                 The artifact was edited without restamping, so any result quoting this version \
                 describes a different corpus.",
                self.manifest.id, self.manifest.version, self.manifest.checksum, actual
            )));
        }
        Ok(())
    }

    /// The record a golden query names, or an error naming the query.
    ///
    /// Queries refer to records by key, so a query pointing at a record that
    /// no longer exists is a broken fixture rather than a silently-failing
    /// assertion.
    pub fn record(&self, key: &str) -> StoreResult<&CorpusRecord> {
        self.records
            .iter()
            .find(|record| record.key == key)
            .ok_or_else(|| StoreError::Corpus(format!("no corpus record has the key {key}")))
    }

    /// Every key a query references but the corpus does not define.
    ///
    /// Checked on load by the artifact's own test rather than at read time:
    /// a dangling reference is an authoring mistake, and it should fail where
    /// it is introduced.
    pub fn dangling_keys(&self) -> Vec<String> {
        let defined: std::collections::HashSet<&str> =
            self.records.iter().map(|r| r.key.as_str()).collect();
        let mut missing = Vec::new();
        for query in &self.queries {
            let referenced = query
                .expect_top
                .iter()
                .chain(query.expect_present.iter())
                .chain(query.expect_absent.iter());
            for key in referenced {
                if !defined.contains(key.as_str()) {
                    missing.push(format!("{}: {key}", query.name));
                }
            }
        }
        missing
    }
}

impl EconomicsSpec {
    /// The runtime economics this spec describes.
    pub fn to_economics(&self) -> StoreResult<Economics> {
        let last_used_at = match &self.last_used_at {
            Some(text) => Some(text.parse().map_err(|error| {
                StoreError::Corpus(format!("last_used_at {text} is not RFC 3339: {error}"))
            })?),
            None => None,
        };
        Ok(Economics {
            use_count: self.use_count,
            last_used_at,
            value_score: self.value_score,
            currency: self.currency,
        })
    }
}

/// Seeding a corpus artifact into a store, and running its golden queries.
///
/// Kept beside the artifact types rather than in [`crate::corpus`]: this path
/// is what ADV-INFRA-007's calibration estate will use against a *deployed*
/// gateway, where the compiled-in fixtures are not available and the
/// deterministic embedder is not the one doing the work.
mod seeding {
    use surrealdb::Connection;
    use totem_core::{
        ActorId, Author, Content, Harness, MemoryRecord, Provenance, RepoId, Scope, ScopeChain,
        SessionId, TeamId,
    };

    use super::{Corpus, CorpusQuery, CorpusRecord};
    use crate::embedding::Embedder;
    use crate::error::{StoreError, StoreResult};
    use crate::memory::RecallQuery;
    use crate::store::Store;

    /// Every seeded record carries this, so a synthetic memory can never be
    /// mistaken for a real one if a corpus reaches a shared instance.
    ///
    /// It matters *more* once the corpus can be loaded into a deployment than
    /// it did when it only ever reached an in-memory test store.
    pub const GENERATOR_TAG: &str = "totem-synthetic-corpus";

    fn parse<T, E: std::fmt::Display>(result: Result<T, E>, what: &str) -> StoreResult<T> {
        result.map_err(|error| StoreError::Corpus(format!("{what}: {error}")))
    }

    impl CorpusRecord {
        /// The writer's resolved scope chain.
        fn writer_chain(&self) -> StoreResult<ScopeChain> {
            let actor = parse(ActorId::new(&self.writer_actor), "writer_actor")?;
            let project = match &self.writer_project {
                Some(repo) => Some(parse(RepoId::new(repo), "writer_project")?),
                None => None,
            };
            let teams: Vec<TeamId> = self
                .writer_teams
                .iter()
                .map(|team| parse(TeamId::new(team), "writer_teams"))
                .collect::<StoreResult<_>>()?;
            Ok(ScopeChain::resolve(&actor, project.as_ref(), &teams))
        }

        fn parsed_scope(&self) -> StoreResult<Scope> {
            parse(self.scope.parse::<Scope>(), "scope")
        }
    }

    impl CorpusQuery {
        /// The reader's resolved scope chain.
        pub fn reader_chain(&self) -> StoreResult<ScopeChain> {
            let actor = parse(ActorId::new(&self.reader_actor), "reader_actor")?;
            let project = match &self.reader_project {
                Some(repo) => Some(parse(RepoId::new(repo), "reader_project")?),
                None => None,
            };
            let teams: Vec<TeamId> = self
                .reader_teams
                .iter()
                .map(|team| parse(TeamId::new(team), "reader_teams"))
                .collect::<StoreResult<_>>()?;
            Ok(ScopeChain::resolve(&actor, project.as_ref(), &teams))
        }
    }

    impl Corpus {
        /// Write every record into `store`, embedded by `embedder`.
        ///
        /// Takes the embedder rather than assuming the deterministic one: a
        /// calibration estate on a deployment is ranked by the real model, and
        /// a corpus seeded with one geometry and queried with another ranks
        /// meaninglessly (ADV-STORE-008).
        pub async fn seed<C: Connection>(
            &self,
            store: &Store<C>,
            embedder: &dyn Embedder,
        ) -> StoreResult<usize> {
            for record in &self.records {
                let mut tags = record.tags.clone();
                tags.push(GENERATOR_TAG.to_string());
                if let Some(cluster) = &record.cluster {
                    tags.push(format!("cluster:{cluster}"));
                }

                let content =
                    crate::embedding::embed(embedder, Content::new(&record.body).with_tags(tags))?;
                let created_at = parse(
                    record.created_at.parse::<chrono::DateTime<chrono::Utc>>(),
                    "created_at",
                )?;
                let provenance = Provenance::new(
                    Author::Agent(parse(ActorId::new(&record.writer_actor), "writer_actor")?),
                    Harness::Curator,
                    parse(SessionId::new("calibration-seed"), "session")?,
                    created_at,
                );
                let mut memory =
                    MemoryRecord::new(record.category, record.parsed_scope()?, content, provenance);
                memory.economics = record.economics.to_economics()?;
                store
                    .memories()
                    .save(&record.writer_chain()?, &memory)
                    .await?;
            }
            Ok(self.records.len())
        }

        /// Run one golden query and return what its reader would see, ranked.
        ///
        /// Non-reinforcing (ADV-GATEWAY-017): an evaluation that meters the
        /// records it retrieves cannot be run twice against one store and
        /// compared, because the second run measures a state the first created.
        pub async fn run_query<C: Connection>(
            &self,
            store: &Store<C>,
            embedder: &dyn Embedder,
            query: &CorpusQuery,
        ) -> StoreResult<Vec<MemoryRecord>> {
            let mut recall = RecallQuery::new()
                .in_categories(query.categories.iter().copied())
                .limit(query.limit);
            if let Some(probe) = &query.probe {
                recall = recall.near(embedder.embed(probe)?)?.top_k(10);
            }
            store
                .memories()
                .recall_observing(&query.reader_chain()?, &recall)
                .await
        }
    }
}

pub use seeding::GENERATOR_TAG;
