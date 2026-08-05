---
advance:
  id: "ADV-CORE-001"
  title: "Workspace scaffold + domain types (memory categories, scopes, provenance)"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-05T06:09:52Z"
  review_time_estimate_minutes: 60
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 88
  risk_flags: ["new_dependency", "public_api"]
  evidence: ["tidy:preparatory", "tdd:red-green", "tests:unit"]
  # Populated by `arrive usage import` / the LiteLLM callback — leave empty when authoring.
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

Stand up the Rust workspace (`crates/totem-core`, with sibling crate slots per
Solution Intent §7) and define the core domain types: the six memory categories
(Episodic, Identity, Knowledge, Context, Instructions, Uncertainty), the scope
enum (`actor` / `project` / `team` / `platform`), the common memory record shape
(identity, content, provenance, economics, governance groups), and provenance
types. See docs/solution-intent.md §2.

## Behavioral Change

After this advance:
- `cargo build` succeeds on a workspace containing `totem-core`.
- Domain types for memory categories, scopes, and provenance exist with
  serde round-trip support and unit tests; category determines lifecycle
  metadata (e.g. Episodic is append-only, Context carries a TTL).
- The record shape marked "variable" in the Solution Intent is firmed up here.

Concretely, the domain now enforces three rules that everything downstream
inherits:

- **Category decides lifecycle, not the caller.** `MemoryCategory::lifecycle()`
  returns mutability, default TTL, decay, review policy, and injection
  priority. `MemoryRecord::revise()` refuses append-only categories, so
  rewriting an episode is a compile-time-reachable error rather than a
  convention.
- **A reader can only name scopes it was granted.** `ScopeChain::resolve()` is
  the only constructor, and it builds the chain from the caller's own actor id,
  their project, and their teams. There is no way to obtain a chain containing
  another actor's private scope.
- **No memory exists without provenance.** `Provenance` has no `Default` and no
  empty constructor; `MemoryRecord::new()` takes it as a required parameter.

`Scope` serialises as its wire form (`actor:ada`, `project:srswart/totem`,
`team:058-totem`, `platform`), so the same text appears in the store, the API,
and the audit log.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: move any `src/**` remnants into the workspace layout (no behavior change)
- [x] test: category/scope/provenance type tests (serde round-trip, lifecycle rules)
- [x] feat: workspace scaffold + `totem-core` domain types

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: domain shape decided here constrains store schema and gateway API;
  wrong category/scope modeling is expensive to change later.
- Risk (`new_dependency`): four external crates enter the workspace —
  `serde`, `chrono`, `uuid`, `thiserror`. All are widely used and none is a
  transport or storage dependency. `Cargo.lock` is committed so hourly cloud
  runs cannot be broken by an upstream release mid-project.
- Risk (`public_api`): every type here is public API for `totem-store` and
  `totem-gateway`. Speculative accessors were deliberately left out — later
  advances add what they actually need rather than inheriting unused surface.
- Rollback: revert the advance branch; no persisted data exists yet.

## Reviewability

`arrive score` reports **88 (Red)**. The work was **not** split, for these
reasons:

- The score is dominated by Size (63 of 88) on a **1,904-line** diff of which
  **445 lines are a generated `Cargo.lock`** and 56 more are manifests and
  `.gitignore` — roughly a quarter of the diff is not hand-reviewed code. The
  hand-written surface is ~677 lines of `src` (of which ~200 are doc comments)
  and 393 lines of tests.
- Novelty (20) is unavoidable: this is the first code in the repository, so
  every file is new by definition. No scaffold advance can score Green.
- The five modules are **mutually dependent**: `record.rs` cannot compile
  without `category`, `scope`, `provenance`, and `ids`. Splitting would land a
  domain model that does not satisfy this advance's own Behavioral Change, and
  `ADV-STORE-001` depends on the complete shape.

Reviewers should read in this order: `scope.rs` (the isolation boundary and the
highest-severity surface), `category.rs` (the lifecycle table), `record.rs`,
then `provenance.rs` and `ids.rs`.

The `concurrency` risk flag `arrive score` reports is a **false positive**: the
scorer matches the substring `sync` in the root `Cargo.toml` comment listing the
future `crates/totem-arrive-sync` slot. No code in this advance is concurrent.
The comment was kept rather than reworded, since dodging a scorer heuristic by
degrading documentation is the wrong trade; the flag is recorded here instead.
Frontmatter `risk_flags` records the two flags that are real
(`new_dependency`, `public_api`).

## Evidence

- [x] tidy:preparatory — `tidy:` commit precedes `test:` and `feat:`, and
      changes only ARRIVE selectors and one doc sentence (no code existed yet).
- [x] tdd:red-green — the `test:` commit was verified RED before implementation:
      `cargo test --workspace` failed with `E0432: unresolved imports` naming all
      25 domain types. The `feat:` commit turned it green.
- [x] tests:unit — 41 tests pass (15 lib unit + 25 integration + 1 doctest):
      `cargo test --workspace`.
- Mutation check (beyond the required evidence): the two security-relevant
  invariants were re-verified by deliberately breaking the implementation.
  Making `ScopeChain::contains` return `true` unconditionally failed
  `resolved_chain_never_contains_another_actors_scope`,
  `resolved_chain_omits_projects_and_teams_the_actor_is_not_in`, and the crate
  doctest; removing the append-only guard from `revise()` failed
  `revising_an_episodic_record_is_refused` and
  `every_category_can_be_written_and_only_episodic_refuses_revision`. Both
  mutations were reverted before commit.
- Not claimed: `ci:passed`. No pipeline ran for this branch, and none was
  observed.
- Not claimed: `tests:integration` in the ARRIVE sense. The files under
  `crates/totem-core/tests/` are Rust integration tests (they exercise the
  public API from outside the crate), but they cross no process or service
  boundary — there is no store or gateway yet.

## CI Evidence Notes

- If CI jobs are enabled, link pipeline evidence (`ci:passed`) from PR/MR and default-branch runs.
- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CORE-001 --status passed`

## Changes Made

### 2026-08-05 - tidy: Drop pre-workspace `src/**` roots and selectors
- arrive/systems/058-totem-core/system.yaml: removed the `src/**` root; the
  workspace lands at `crates/**` in this advance.
- arrive/systems/058-totem-core/components/core.yaml: removed the `src/**`
  path selector, leaving `crates/totem-core/**`.
- docs/arrive-decomposition-gaps.md: corrected the note that said `src/**` was
  "retained until the workspace lands".
- No `src/` tree ever existed in the repository, so this changes no behaviour.

### 2026-08-05 - test: Category lifecycle, scope resolution, and record shape
- Cargo.toml: workspace with `resolver = "3"`, edition 2024, shared dependency
  and lint tables; sibling crate slots documented as comments with the advance
  that will add each one.
- Cargo.lock: committed so cloud runs are not broken by upstream releases.
- crates/totem-core/Cargo.toml: the crate manifest.
- crates/totem-core/src/lib.rs: empty crate for the tests to compile against.
- crates/totem-core/tests/scope_isolation.rs: 10 tests — wire-form round trip,
  rejection of malformed scopes, id validation, chain ordering, and the guard
  that a chain resolved for one actor never contains another actor's scope.
- crates/totem-core/tests/domain_types.rs: 15 tests — six categories, the
  per-category lifecycle table, provenance carried verbatim, `derived_from`
  links, fresh economics, human-gated review, refused episodic revision, TTL
  expiry, and JSON round-trip.
- .gitignore: ignore `/target/`.

### 2026-08-05 - feat: Totem core domain model
- crates/totem-core/src/category.rs: `MemoryCategory` (the six categories),
  `Mutability`, `ReviewPolicy`, and `CategoryLifecycle`; `lifecycle()` maps each
  category to its rules, `is_append_only()` is the guard `record.rs` uses.
- crates/totem-core/src/scope.rs: `Scope` with `Display`/`FromStr`/serde over
  the wire form, `ScopeParseError`, and `ScopeChain` with `resolve()`,
  `scopes()`, `contains()`, `precedence_of()`.
- crates/totem-core/src/provenance.rs: `Author` (human/agent/curator),
  `Harness`, and `Provenance` with required author, harness, session, and
  timestamp plus optional `at_turn()` and `derived_from()`.
- crates/totem-core/src/record.rs: `MemoryRecord` in the five documented groups,
  plus `Content`, `Economics`, `Governance`, `MemoryStatus`, `ReviewState`,
  `SubjectRef`/`SubjectKind`, and `LifecycleError`; `revise()` enforces
  append-only and `expires_at()` derives expiry from the category TTL.
- crates/totem-core/src/ids.rs: validated `ActorId`, `RepoId`, `TeamId`,
  `SessionId` newtypes (rejecting empty and untrimmed values) and `MemoryId`.
- crates/totem-core/src/lib.rs: crate documentation stating the three
  load-bearing rules, a compiling doctest, `#![warn(missing_docs)]`, and the
  public re-exports.

## Check for Understanding

1. `ScopeChain::resolve()` in `crates/totem-core/src/scope.rs` is the only way
   to build a chain, and its `scopes` field is private. What would be lost if
   `ScopeChain` instead exposed a public `from_scopes(Vec<Scope>)` constructor,
   and which test in `tests/scope_isolation.rs` would stop being meaningful?
2. `Scope` serialises as `actor:ada` rather than as a tagged struct. Given
   `FromStr` in `scope.rs` splits on the first `:`, what happens to a repo id
   that itself contains a colon, and why does `ActorId::new` rejecting untrimmed
   values matter for isolation rather than merely for tidiness?
3. `MemoryRecord::revise()` refuses append-only categories, but `content` is a
   public field on the struct. Where does that leave the append-only invariant,
   and which layer is stated in `crates/totem-core/src/lib.rs` as the place that
   actually enforces it?
4. `MemoryCategory::lifecycle()` in `category.rs` gives Episodic
   `decays: false` and `injection_priority: 10`, while Instructions gets
   `decays: false` and `100`. Explain why the audit substrate is both
   non-decaying and lowest-priority for injection.
5. `Provenance` implements neither `Default` nor an empty builder. Name the
   specific auditability property (see docs/project-brief.md, G3) this protects,
   and say whether `MemoryRecord::new()` could satisfy it if `provenance` were
   an `Option`.
6. `Governance::initial()` sets `ReviewState::Pending` for Instructions and
   Uncertainty but `NotRequired` for Knowledge. Which field of
   `CategoryLifecycle` drives that, and what does it imply for the promotion
   engine that `ADV-CORE-003` will build on top?
7. The `## Reviewability` section argues this Red-scoring advance should not be
   split. Which single file would you split out first if you disagreed, and what
   would break in `crates/totem-core/src/record.rs` if you did?
