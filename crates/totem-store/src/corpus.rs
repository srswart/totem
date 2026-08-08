//! Synthetic memory corpus for the evaluation advances (ADV-STORE-005).
//!
//! [`ADV-CORE-005`](../../../arrive/systems/058-totem-core/advances/ADV-CORE-005.md)
//! (quality), [`ADV-GATEWAY-006`](../../../arrive/systems/058-totem-core/advances/ADV-GATEWAY-006.md)
//! (security), and [`ADV-GATEWAY-007`](../../../arrive/systems/058-totem-core/advances/ADV-GATEWAY-007.md)
//! (performance) all need a seeded memory estate to run against instead of
//! reconstructing one from scratch. This module is that estate: a fixed,
//! reproducible set of records spanning every category and every scope tier,
//! plus the golden queries and leak-bait fixtures the evaluations score
//! against.
//!
//! **Synthetic data only.** Every identity here (`corpus-nova`,
//! `corpus/rocket`, ...) is fictional; none names a real actor, repo, or
//! team. Every record additionally carries [`GENERATOR_TAG`], so a synthetic
//! memory can never be mistaken for a real one if a corpus ever leaks into a
//! shared instance.
//!
//! **Reset is "build a fresh store", not "delete the rows".** Episodic rows
//! are append-only at the schema level (`schema.rs`'s
//! `memory_episodic_no_delete` event refuses `DELETE` outright), so a corpus
//! containing Episodic fixtures cannot be wiped in place. [`seeded_in_memory`]
//! is the deterministic reset path: a brand new embedded instance, migrated
//! and reseeded, every time.
//!
//! Content embeddings use [`DeterministicEmbedder`]: real cosine geometry
//! with no network dependency, so the golden queries below are genuinely
//! exercising vector recall, not asserting a hand-computed distance.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use surrealdb::Connection;
use surrealdb::engine::local::Db;
use totem_core::{
    ActorId, Author, Content, Economics, Harness, MemoryCategory, MemoryRecord, Provenance, RepoId,
    Scope, ScopeChain, SessionId, TeamId,
};

use crate::embedding::{DeterministicEmbedder, embed};
use crate::error::StoreResult;
use crate::memory::RecallQuery;
use crate::store::Store;

/// Every record this module writes carries this tag, so a synthetic memory
/// can never be mistaken for a real one if a corpus leaks into a shared
/// instance.
pub const GENERATOR_TAG: &str = "totem-synthetic-corpus";

/// A fictional actor: the corpus's primary developer identity.
pub const NOVA: &str = "corpus-nova";
/// A fictional actor: a second developer, used to prove isolation from
/// [`NOVA`]'s private scope.
pub const JUNIPER: &str = "corpus-juniper";
/// A fictional actor: an agent identity belonging to [`PLATFORM_TEAM`].
pub const ATLAS: &str = "corpus-atlas";
/// A fictional enrolled repository.
pub const ROCKET: &str = "corpus/rocket";
/// A second fictional enrolled repository, so the corpus exercises more than
/// one project.
pub const BEACON: &str = "corpus/beacon";
/// A fictional team scope shared across both fictional repositories.
pub const PLATFORM_TEAM: &str = "corpus-platform-team";

/// The actor whose actor-scope copy wins the precedence pair
/// ([`PRECEDENCE_BODY`]) — [`NOVA`], reading with the [`ROCKET`] project in
/// its chain.
pub const PRECEDENCE_READER: &str = NOVA;
/// The body written at both `actor:corpus-nova` and `project:corpus/rocket`,
/// to prove the narrower scope wins the store's merge-by-precedence rule.
pub const PRECEDENCE_BODY: &str = "The rocket project's default review SLA is two business days.";

/// One side of the contested Uncertainty pair.
pub const CONTESTED_A: &str = "Team decided to deprecate the rocket project's v1 API by Q4.";
/// The other side of the contested Uncertainty pair.
pub const CONTESTED_B: &str =
    "Team decided to keep the rocket project's v1 API supported indefinitely.";

/// The well-used incumbent of the economics pair: a Knowledge record with a
/// long, successful history behind it — cited ten times, recalled often, and
/// recalled recently, so all three non-relevance terms sit at their maximum.
///
/// It is about deploys, like [`ECONOMICS_CHALLENGER`], so the pair is a
/// genuine contest between a related-but-not-asked record and an exact
/// answer, rather than the trivial case of an obviously unrelated one.
pub const ECONOMICS_INCUMBENT: &str =
    "The rocket project deploys from main every weekday afternoon.";

/// The challenger of the economics pair: never recalled, never cited, and an
/// exact match for the query that asks for it.
///
/// Nothing but relevance argues for it. If ranking returns the incumbent
/// first, history has outweighed what was actually asked.
///
/// The query probes this text verbatim rather than paraphrasing it, because
/// [`DeterministicEmbedder`] has no semantics — a reworded probe would be
/// orthogonal to every record here, including this one, and the query would
/// measure nothing. Paraphrase is exercised against the real embedder on the
/// deployed instance, which is where ADV-CORE-008's golden-query evidence is
/// taken.
pub const ECONOMICS_CHALLENGER: &str =
    "Rocket's canary deploy holds at ten percent of traffic for fifteen minutes.";

const NEAR_DUP_A: &str =
    "The rocket project's staging database runs Postgres 16.3 with the pgvector extension enabled.";
const NEAR_DUP_B: &str = "Rocket's staging DB runs Postgres 16.3, pgvector extension turned on.";
const INSTRUCTIONS_PROJECT_FACT: &str =
    "Rocket project rule: all schema changes land as their own reviewable advance.";

/// How many records [`seed`] wrote, broken down by category — the report a
/// caller (a CLI command, a test, an evaluation harness) checks to confirm
/// the corpus landed as expected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusReport {
    /// The total number of records written.
    pub total: usize,
    /// Records written per category, every category represented.
    pub by_category: Vec<(MemoryCategory, usize)>,
}

/// One "leak bait" pair: byte-identical content written at two different
/// private actor scopes, so a security evaluation
/// ([`ADV-GATEWAY-006`](../../../arrive/systems/058-totem-core/advances/ADV-GATEWAY-006.md))
/// can prove neither owner's reader chain ever returns the other's copy —
/// not the body (identical either way) but the *count* and the *provenance*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeakBaitPair {
    /// A short label for the pair.
    pub name: &'static str,
    /// The category both copies are written as.
    pub category: MemoryCategory,
    /// The identical body both owners' copies carry.
    pub body: &'static str,
    /// The first owner's actor id.
    pub owner_a: &'static str,
    /// The second owner's actor id.
    pub owner_b: &'static str,
}

/// One golden recall query: a reader identity, what to recall, and the
/// expected result — the input [`ADV-GATEWAY-008`](../../../arrive/systems/058-totem-core/advances/ADV-GATEWAY-008.md)'s
/// recall-quality scorer runs against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenQuery {
    /// A short label for the query.
    pub name: &'static str,
    /// The reader's actor id.
    pub reader_actor: &'static str,
    /// The project in the reader's scope chain, if any.
    pub reader_project: Option<&'static str>,
    /// The teams in the reader's scope chain.
    pub reader_teams: &'static [&'static str],
    /// Restrict recall to these categories; empty means every category.
    pub categories: &'static [MemoryCategory],
    /// If set, the query text embedded and used as the vector probe.
    pub probe_text: Option<&'static str>,
    /// If set, the body expected to rank first.
    pub expected_top: Option<&'static str>,
    /// Bodies that must appear somewhere in the result set.
    pub must_appear: &'static [&'static str],
}

/// One fixture record: what to write, at what scope, and who writes it.
struct Fixture {
    category: MemoryCategory,
    scope: Scope,
    writer: ScopeChain,
    body: &'static str,
    tags: Vec<String>,
    provenance: Provenance,
    economics: Economics,
}

impl Fixture {
    /// Give this record a history: the non-relevance half of the ranking
    /// formula, which every other fixture leaves at [`Economics::fresh`].
    ///
    /// A corpus where every record is pristine cannot distinguish a ranker
    /// that weighs history correctly from one that ignores the query
    /// entirely, because the three non-relevance terms are then constant
    /// across the whole estate and cancel (ADV-CORE-008).
    fn with_economics(mut self, economics: Economics) -> Self {
        self.economics = economics;
        self
    }
}

fn actor(id: &str) -> ActorId {
    ActorId::new(id).expect("corpus actor ids are valid")
}

fn repo(id: &str) -> RepoId {
    RepoId::new(id).expect("corpus repo ids are valid")
}

fn team(id: &str) -> TeamId {
    TeamId::new(id).expect("corpus team ids are valid")
}

/// Resolve a reader's (or writer's) scope chain from plain identifiers —
/// the same construction [`seed`] uses internally, exposed so tests and
/// evaluation harnesses can build the identical chain a golden query or
/// leak-bait check reads with.
pub fn reader_chain(actor_id: &str, project: Option<&str>, teams: &[&str]) -> ScopeChain {
    let teams: Vec<TeamId> = teams.iter().map(|t| team(t)).collect();
    let project = project.map(repo);
    ScopeChain::resolve(&actor(actor_id), project.as_ref(), &teams)
}

fn fixture(
    category: MemoryCategory,
    scope: Scope,
    writer: &ScopeChain,
    body: &'static str,
    tags: &[&str],
    provenance: Provenance,
) -> Fixture {
    let mut all_tags: Vec<String> = tags.iter().map(|tag| tag.to_string()).collect();
    all_tags.push(GENERATOR_TAG.to_string());
    Fixture {
        category,
        scope,
        writer: writer.clone(),
        body,
        tags: all_tags,
        provenance,
        economics: Economics::fresh(),
    }
}

/// The two leak-bait pairs the corpus seeds: identical Knowledge and
/// Identity content at [`NOVA`]'s and [`JUNIPER`]'s private actor scopes.
pub fn leak_bait_pairs() -> Vec<LeakBaitPair> {
    vec![
        LeakBaitPair {
            name: "unreleased_pricing_change",
            category: MemoryCategory::Knowledge,
            body: "The unreleased Q3 pricing change moves the enterprise tier to $4,200/mo.",
            owner_a: NOVA,
            owner_b: JUNIPER,
        },
        LeakBaitPair {
            name: "personal_emergency_contact",
            category: MemoryCategory::Identity,
            body: "This actor's personal emergency contact number ends in 0148.",
            owner_a: NOVA,
            owner_b: JUNIPER,
        },
    ]
}

/// The golden query set: recall scenarios with a known-correct answer.
pub fn golden_queries() -> Vec<GoldenQuery> {
    vec![
        GoldenQuery {
            name: "vector_recall_ranks_the_matching_instruction_first",
            reader_actor: NOVA,
            reader_project: Some(ROCKET),
            reader_teams: &[],
            categories: &[MemoryCategory::Instructions],
            probe_text: Some(INSTRUCTIONS_PROJECT_FACT),
            expected_top: Some(INSTRUCTIONS_PROJECT_FACT),
            must_appear: &[],
        },
        GoldenQuery {
            name: "near_duplicates_both_surface_for_dedupe_input",
            reader_actor: NOVA,
            reader_project: Some(ROCKET),
            reader_teams: &[],
            categories: &[MemoryCategory::Knowledge],
            probe_text: Some(NEAR_DUP_A),
            expected_top: None,
            must_appear: &[NEAR_DUP_A, NEAR_DUP_B],
        },
        GoldenQuery {
            name: "an_exact_match_outranks_a_well_used_incumbent",
            reader_actor: NOVA,
            reader_project: Some(ROCKET),
            reader_teams: &[],
            categories: &[MemoryCategory::Knowledge],
            probe_text: Some(ECONOMICS_CHALLENGER),
            expected_top: Some(ECONOMICS_CHALLENGER),
            must_appear: &[],
        },
        GoldenQuery {
            name: "contested_pair_keeps_both_sides_visible",
            reader_actor: NOVA,
            reader_project: Some(ROCKET),
            reader_teams: &[],
            categories: &[MemoryCategory::Uncertainty],
            probe_text: None,
            expected_top: None,
            must_appear: &[CONTESTED_A, CONTESTED_B],
        },
    ]
}

/// Run one golden query against a seeded store, returning the ranked,
/// scope-resolved result the query's reader would see.
pub async fn run_golden_query<C: Connection>(
    store: &Store<C>,
    query: &GoldenQuery,
) -> StoreResult<Vec<MemoryRecord>> {
    let reader = reader_chain(query.reader_actor, query.reader_project, query.reader_teams);
    let mut recall = RecallQuery::new().in_categories(query.categories.iter().copied());
    if let Some(text) = query.probe_text {
        let probe = embed(&DeterministicEmbedder::new(), Content::new(text))?
            .embedding
            .expect("embed always attaches a vector");
        recall = recall.near(probe)?.top_k(5);
    }
    store.memories().recall(&reader, &recall).await
}

fn provenance(
    author: Author,
    harness: Harness,
    session: &'static str,
    at: &'static str,
) -> Provenance {
    Provenance::new(
        author,
        harness,
        SessionId::new(session).expect("corpus session ids are valid"),
        at.parse::<DateTime<Utc>>()
            .expect("corpus timestamps are valid RFC 3339"),
    )
}

/// The full fixture list: the 6×4 category×scope grid, plus the leak-bait,
/// near-duplicate, aged, expired, contested, and precedence scenarios
/// (ADV-STORE-005, "Required Scenarios").
fn fixtures() -> Vec<Fixture> {
    let nova_actor = reader_chain(NOVA, None, &[]);
    let nova_project = reader_chain(NOVA, Some(ROCKET), &[]);
    let juniper_actor = reader_chain(JUNIPER, None, &[]);
    let atlas_team = reader_chain(ATLAS, Some(BEACON), &[PLATFORM_TEAM]);

    let human = |id: &str| Author::Human(actor(id));
    let agent = |id: &str| Author::Agent(actor(id));
    let curator = |id: &str| Author::Curator(actor(id));

    let mut records = vec![
        // --- The 6x4 category x scope-tier grid -----------------------
        fixture(
            MemoryCategory::Episodic,
            Scope::Actor(actor(NOVA)),
            &nova_actor,
            "Nova asked the agent to summarize yesterday's deploy; agent replied with the rollout timeline.",
            &["grid"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-episodic-actor",
                "2026-07-01T09:00:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Episodic,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            "Turn log: rocket project agent proposed a rollback plan after the staging alert fired.",
            &["grid"],
            provenance(
                agent(NOVA),
                Harness::ClaudeCode,
                "corpus-session-episodic-project",
                "2026-07-01T09:05:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Episodic,
            Scope::Team(team(PLATFORM_TEAM)),
            &atlas_team,
            "Turn log: platform team agent walked through the shared incident runbook during a sync.",
            &["grid"],
            provenance(
                agent(ATLAS),
                Harness::CloudAgent,
                "corpus-session-episodic-team",
                "2026-07-01T09:10:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Episodic,
            Scope::Platform,
            &atlas_team,
            "Turn log: platform curator recorded a cross-repo advance status sweep.",
            &["grid"],
            provenance(
                curator(ATLAS),
                Harness::Curator,
                "corpus-session-episodic-platform",
                "2026-07-01T09:15:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Identity,
            Scope::Actor(actor(NOVA)),
            &nova_actor,
            "Nova is a backend engineer on the rocket project; timezone UTC-5.",
            &["grid"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-identity-actor",
                "2026-07-01T09:20:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Identity,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            "The rocket project's on-call rotation is Nova (primary), Juniper (secondary).",
            &["grid"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-identity-project",
                "2026-07-01T09:25:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Identity,
            Scope::Team(team(PLATFORM_TEAM)),
            &atlas_team,
            "Atlas is the platform team's automation agent identity, distinct from any human actor.",
            &["grid"],
            provenance(
                curator(ATLAS),
                Harness::Curator,
                "corpus-session-identity-team",
                "2026-07-01T09:30:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Identity,
            Scope::Platform,
            &atlas_team,
            "The platform-wide actor directory lists three enrolled identities: corpus-nova, corpus-juniper, corpus-atlas.",
            &["grid"],
            provenance(
                curator(ATLAS),
                Harness::Curator,
                "corpus-session-identity-platform",
                "2026-07-01T09:35:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Knowledge,
            Scope::Actor(actor(NOVA)),
            &nova_actor,
            "Nova prefers terse commit messages and squash-merges her own branches.",
            &["grid"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-knowledge-actor",
                "2026-07-01T09:40:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Knowledge,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            "The rocket project's staging environment runs Postgres 16.3.",
            &["grid"],
            provenance(
                agent(NOVA),
                Harness::ClaudeCode,
                "corpus-session-knowledge-project",
                "2026-07-01T09:45:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Knowledge,
            Scope::Team(team(PLATFORM_TEAM)),
            &atlas_team,
            "The platform team standard is trunk-based development with short-lived branches.",
            &["grid"],
            provenance(
                agent(ATLAS),
                Harness::CloudAgent,
                "corpus-session-knowledge-team",
                "2026-07-01T09:50:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Knowledge,
            Scope::Platform,
            &atlas_team,
            "Platform-wide: every enrolled repo must expose an ARRIVE landscape within five minutes of a merge.",
            &["grid"],
            provenance(
                curator(ATLAS),
                Harness::Curator,
                "corpus-session-knowledge-platform",
                "2026-07-01T09:55:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Context,
            Scope::Actor(actor(NOVA)),
            &nova_actor,
            "Nova is currently investigating a flaky test in the rocket auth module.",
            &["grid"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-context-actor",
                "2026-08-05T09:00:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Context,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            "The rocket project is mid-migration from REST to streamable-HTTP MCP this week.",
            &["grid"],
            provenance(
                agent(NOVA),
                Harness::ClaudeCode,
                "corpus-session-context-project",
                "2026-08-05T09:05:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Context,
            Scope::Team(team(PLATFORM_TEAM)),
            &atlas_team,
            "The platform team is running a bake week before the next store schema migration.",
            &["grid"],
            provenance(
                agent(ATLAS),
                Harness::CloudAgent,
                "corpus-session-context-team",
                "2026-08-05T09:10:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Context,
            Scope::Platform,
            &atlas_team,
            "Platform-wide maintenance window is scheduled for this weekend.",
            &["grid"],
            provenance(
                curator(ATLAS),
                Harness::Curator,
                "corpus-session-context-platform",
                "2026-08-05T09:15:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Instructions,
            Scope::Actor(actor(NOVA)),
            &nova_actor,
            "Nova's standing instruction: never merge without a green CI run, even for hotfixes.",
            &["grid"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-instructions-actor",
                "2026-07-01T10:00:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Instructions,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            INSTRUCTIONS_PROJECT_FACT,
            &["grid"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-instructions-project",
                "2026-07-01T10:05:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Instructions,
            Scope::Team(team(PLATFORM_TEAM)),
            &atlas_team,
            "Platform team rule: curator actions must always be reversible and logged.",
            &["grid"],
            provenance(
                human(ATLAS),
                Harness::Console,
                "corpus-session-instructions-team",
                "2026-07-01T10:10:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Instructions,
            Scope::Platform,
            &atlas_team,
            "Platform-wide rule: scope isolation is enforced at the store layer, never the application layer.",
            &["grid"],
            provenance(
                human(ATLAS),
                Harness::Console,
                "corpus-session-instructions-platform",
                "2026-07-01T10:15:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Uncertainty,
            Scope::Actor(actor(NOVA)),
            &nova_actor,
            "Unresolved: whether Nova's local override of the lint config should be team-wide.",
            &["grid"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-uncertainty-actor",
                "2026-07-01T10:20:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Uncertainty,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            "Unresolved: whether the rocket project's staging alert threshold is miscalibrated.",
            &["grid"],
            provenance(
                agent(NOVA),
                Harness::ClaudeCode,
                "corpus-session-uncertainty-project",
                "2026-07-01T10:25:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Uncertainty,
            Scope::Team(team(PLATFORM_TEAM)),
            &atlas_team,
            "Unresolved: whether the platform team should own the recall-quality scorer or gateway should.",
            &["grid"],
            provenance(
                agent(ATLAS),
                Harness::CloudAgent,
                "corpus-session-uncertainty-team",
                "2026-07-01T10:30:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Uncertainty,
            Scope::Platform,
            &atlas_team,
            "Unresolved: whether platform-wide memory retention should default to 90 or 180 days.",
            &["grid"],
            provenance(
                curator(ATLAS),
                Harness::Curator,
                "corpus-session-uncertainty-platform",
                "2026-07-01T10:35:00Z",
            ),
        ),
        // --- Near-duplicate Knowledge (dedupe input) --------------------
        fixture(
            MemoryCategory::Knowledge,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            NEAR_DUP_A,
            &["near-duplicate"],
            provenance(
                agent(NOVA),
                Harness::ClaudeCode,
                "corpus-session-near-dup-a",
                "2026-07-02T09:00:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Knowledge,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            NEAR_DUP_B,
            &["near-duplicate"],
            provenance(
                agent(NOVA),
                Harness::ClaudeCode,
                "corpus-session-near-dup-b",
                "2026-07-02T09:05:00Z",
            ),
        ),
        // --- Aged Knowledge (currency scoring) --------------------------
        fixture(
            MemoryCategory::Knowledge,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            "The rocket project originally targeted a Q1 2025 GA date.",
            &["aged"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-aged",
                "2025-01-01T00:00:00Z",
            ),
        ),
        // --- Expired Context (TTL scoring) ------------------------------
        fixture(
            MemoryCategory::Context,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            "The rocket project's temporary feature flag rollout window closes shortly after this note.",
            &["expired"],
            provenance(
                agent(NOVA),
                Harness::ClaudeCode,
                "corpus-session-expired",
                "2025-01-01T00:00:00Z",
            ),
        ),
        // --- Contested Uncertainty pair ----------------------------------
        fixture(
            MemoryCategory::Uncertainty,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            CONTESTED_A,
            &["contested-pair", "contested:v1-api"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-contested-a",
                "2026-07-03T09:00:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Uncertainty,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            CONTESTED_B,
            &["contested-pair", "contested:v1-api"],
            provenance(
                human(JUNIPER),
                Harness::Console,
                "corpus-session-contested-b",
                "2026-07-03T09:05:00Z",
            ),
        ),
        // --- Economics pair (history must not outweigh relevance) --------
        //
        // Both Knowledge, both at project scope, so `category_weight` is
        // identical and cannot be what separates them. The only differences
        // are the query's distance and the records' histories — which is
        // exactly the contest ADV-CORE-008 is about.
        fixture(
            MemoryCategory::Knowledge,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            ECONOMICS_INCUMBENT,
            &["economics", "economics:incumbent"],
            provenance(
                agent(NOVA),
                Harness::ClaudeCode,
                "corpus-session-economics-incumbent",
                "2026-07-06T09:00:00Z",
            ),
        )
        .with_economics(Economics {
            // Ten citations at CITATION_BOOST (+0.2) each, on top of the
            // neutral 1.0 — a plausible ceiling for a genuinely load-bearing
            // memory in a year-old estate, not an extreme.
            value_score: 3.0,
            use_count: 47,
            // Recalled yesterday, so currency has not decayed at all.
            last_used_at: Some(
                "2026-08-07T09:00:00Z"
                    .parse()
                    .expect("corpus timestamps are valid RFC 3339"),
            ),
            currency: 1.0,
        }),
        fixture(
            MemoryCategory::Knowledge,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            ECONOMICS_CHALLENGER,
            &["economics", "economics:challenger"],
            provenance(
                agent(NOVA),
                Harness::ClaudeCode,
                "corpus-session-economics-challenger",
                "2026-07-06T09:05:00Z",
            ),
        ),
        // --- Precedence pair (actor scope must win project scope) ------
        fixture(
            MemoryCategory::Knowledge,
            Scope::Actor(actor(NOVA)),
            &nova_actor,
            PRECEDENCE_BODY,
            &["precedence"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-precedence-actor",
                "2026-07-04T09:00:00Z",
            ),
        ),
        fixture(
            MemoryCategory::Knowledge,
            Scope::Project(repo(ROCKET)),
            &nova_project,
            PRECEDENCE_BODY,
            &["precedence"],
            provenance(
                human(NOVA),
                Harness::Console,
                "corpus-session-precedence-project",
                "2026-07-04T08:00:00Z",
            ),
        ),
    ];

    // Both fixed leak-bait pairs (`leak_bait_pairs`) run NOVA against
    // JUNIPER, so their private actor-scope chains cover both sides.
    for pair in leak_bait_pairs() {
        records.push(fixture(
            pair.category,
            Scope::Actor(actor(pair.owner_a)),
            &nova_actor,
            pair.body,
            &["leak-bait", pair.name],
            provenance(
                human(pair.owner_a),
                Harness::Console,
                "corpus-session-leak-bait-a",
                "2026-07-05T09:00:00Z",
            ),
        ));
        records.push(fixture(
            pair.category,
            Scope::Actor(actor(pair.owner_b)),
            &juniper_actor,
            pair.body,
            &["leak-bait", pair.name],
            provenance(
                human(pair.owner_b),
                Harness::Console,
                "corpus-session-leak-bait-b",
                "2026-07-05T09:05:00Z",
            ),
        ));
    }

    records
}

/// Seed the synthetic corpus into `store`. Intended for a fresh, migrated
/// store — an embedded or dedicated test database, never a shared
/// deployment (ADV-STORE-005, "Setup, Reset, and Cleanup").
pub async fn seed<C: Connection>(store: &Store<C>) -> StoreResult<CorpusReport> {
    let embedder = DeterministicEmbedder::new();
    let mut by_category: BTreeMap<MemoryCategory, usize> = BTreeMap::new();

    for record in fixtures() {
        let content = embed(&embedder, Content::new(record.body).with_tags(record.tags))?;
        let mut memory =
            MemoryRecord::new(record.category, record.scope, content, record.provenance);
        memory.economics = record.economics;
        store.memories().save(&record.writer, &memory).await?;
        *by_category.entry(record.category).or_insert(0) += 1;
    }

    Ok(CorpusReport {
        total: by_category.values().sum(),
        by_category: by_category.into_iter().collect(),
    })
}

/// Build a fresh, migrated, seeded in-memory store — the deterministic
/// "reset" path. Rebuilding the whole database rather than deleting rows in
/// place, since the schema refuses to delete the corpus's own Episodic
/// fixtures (see this module's doc comment).
pub async fn seeded_in_memory() -> StoreResult<(Store<Db>, CorpusReport)> {
    let store = Store::in_memory().await?;
    store.migrate().await?;
    let report = seed(&store).await?;
    Ok((store, report))
}
