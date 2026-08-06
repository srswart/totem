---
advance:
  id: "ADV-STORE-005"
  title: "Enablement: synthetic memory corpus (test data for evaluations)"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store", "core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T18:34:35Z"
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: ["https://github.com/srswart/totem/pull/41"]
  external_refs: []
  reviewability_score: 57
  risk_flags: ["public_api"]
  evidence:
    ["profile:selected-practices", "enablement:artifact", "tests:integration"]
  practices:
    tidy_first:
      status: not_applicable
      rationale: "No production_code work product declared; nothing existed to tidy before this module was added."
    tdd:
      status: not_applicable
      rationale: "work_products is test_data, not production_code (arrive-advance-profiles.md), so tdd:red-green is not claimed even though tests/corpus.rs was committed before src/corpus.rs existed and could not compile without it. test_data_provenance (below) is what the profile asks for instead."
    test_data_provenance:
      status: applied
      rationale: "Every fixture carries GENERATOR_TAG; every identity (corpus-nova, corpus/rocket, ...) is fictional; reset is seeded_in_memory — a fresh store every call, since Episodic rows are schema-append-only and cannot be cleared in place."
  model_usage: []
  schema_version: 2
  mode: enablement
  facets: [software, quality, security]
  work_products: [test_data]
  status: complete
---

## Objective

Build the synthetic memory corpus that the evaluation advances
(ADV-CORE-005 quality, ADV-GATEWAY-006 security, ADV-GATEWAY-007 performance)
run against: a seeded memory estate spanning all six categories and all four
scopes, with golden recall queries and expected results, plus adversarial
fixtures (near-duplicates, contradictions, cross-scope lookalikes designed to
surface scope leaks).

## Outcome

After this advance:
- One command seeds a reproducible corpus into a fresh store: multiple actors,
  projects, and a team/platform layer; per-category lifecycle variety (aged
  Knowledge, expired Context, contested Uncertainty pairs).
- Golden query set with expected ranked results exists for recall-quality
  scoring; leak-bait fixtures exist for security evaluation.
- Reset/cleanup is deterministic so evaluations are repeatable.

## Planned Work

- [x] branch: create or confirm feature branch for this advance
- [x] define required scenarios with the evaluation advances' needs as input
- [x] feat: corpus generator + seed/reset commands — `totem_store::corpus::seed`
      / `seeded_in_memory` (a library call, not a standalone CLI binary; see
      Corrections below)
- [x] golden queries + expected results checked in beside the generator —
      `golden_queries()`, `leak_bait_pairs()`, and `run_golden_query()` in
      `crates/totem-store/src/corpus.rs`

## Bug Fixes

- [ ] None yet

## Corrections to the Advance

- **"One command" is a library call, not a terminal command.** The Outcome
  bullet says "one command seeds a reproducible corpus into a fresh store."
  What was built is `totem_store::corpus::seed(&store)` /
  `totem_store::corpus::seeded_in_memory()` — the one call an evaluation
  harness makes, proven end-to-end by `tests/corpus.rs`. There is no
  standalone `totem-*` binary a human runs at a terminal. The immediate
  consumers (ADV-CORE-005, ADV-GATEWAY-006, ADV-GATEWAY-008) are all Rust
  code that will call the library directly; a thin CLI wrapper is a small
  addition any of them — or a future workstation advance — can add on top of
  this module without touching the generator itself. Recorded here rather
  than silently narrowed.
- **Golden queries assert top-1 or membership, not full multi-position rank.**
  `expected_top` guarantees rank-1 for one query
  (`vector_recall_ranks_the_matching_instruction_first`); the other two assert
  unordered membership (`must_appear`) rather than a full ranked order. Full
  precision@k / expected-rank scoring across more than the top result is
  ADV-GATEWAY-008's own construction on top of these fixtures — its advance
  text already frames it that way ("recall-quality scorer... reports ranking
  metrics"), so this is the intended division of work, not a gap discovered
  after the fact.

## Required Scenarios

- All six categories × all four scopes, incl. same-content memories at
  different scopes (leak bait) and near-duplicate Knowledge (dedupe input).
- Aged/decayed memories for currency scoring; contested pairs for Uncertainty.
- Multiple actors/projects to exercise scope-chain merge and precedence.

## Provenance and Privacy

- Synthetic data only — no real session content, names, or repo data. Every
  record is generator-tagged so a synthetic memory can never be mistaken for
  a real one if a corpus leaks into a shared instance.

## Setup, Reset, and Cleanup

- Seed: `totem_store::corpus::seed(&store)` against a migrated store — an
  embedded (`kv-mem`) or dedicated test database, never a shared deployment.
- Reset: `totem_store::corpus::seeded_in_memory()` — builds a brand-new
  embedded instance, migrates it, and seeds it, rather than deleting rows in
  an existing one. Episodic rows are schema-level append-only
  (`memory_episodic_no_delete` refuses `DELETE`), so a corpus containing
  Episodic fixtures cannot be cleared in place; a fresh store is the only
  deterministic reset.
- Cleanup: dropping the (in-memory or on-disk) instance is complete cleanup;
  nothing here writes outside the store it was given.

## Risk + Rollback

- Risk: corpus not representative of real memory shapes — refresh it from
  anonymized patterns once real usage exists.
- Risk: the `DeterministicEmbedder`'s trigram-hash vectors give real cosine
  geometry but no semantic meaning (documented in `embedding.rs`), so the
  golden queries prove recall *mechanics* work, not that a semantically
  meaningful query would rank the same way. `ADV-CORE-005` inherits this
  caveat when it runs a quality evaluation against this corpus.
- Rollback: drop the test database; the generator is deterministic in shape
  (same fixtures every call), though not byte-identical (`MemoryId` and
  `access_log` entries mint fresh UUIDs/timestamps each run by design).

## Evidence

- [x] profile:selected-practices — `test_data_provenance` applied;
      `tidy_first`/`tdd` recorded `not_applicable` with rationale in the
      frontmatter `practices` block.
- [x] enablement:artifact — `crates/totem-store/src/corpus.rs` (the
      generator, golden queries, and leak-bait fixtures) plus
      `crates/totem-store/tests/corpus.rs` (the proof it behaves as
      documented).
- [x] tests:integration — `cargo test --workspace`: 344 tests pass (incl.
      doctests), of which 9 are this advance's own (`tests/corpus.rs`); zero
      regressions in the other 335.
- Not claimed: `tdd:red-green` — see `practices.tdd` above.
- Not claimed: `ci:passed` — no pipeline result has been observed for this
  branch yet.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-STORE-005 --status passed`
- Checks run locally on the branch: `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `arrive doctor artifacts`, `arrive plan check`, `arrive check --strict`,
  `arrive score` — all green.

## Reviewability

`arrive score` reports **57 [YELLOW]** (size 53, novelty 4, risk 0) measured
against the sub-branch's own base (the claim commit). The change is kept
whole rather than split further, because the natural tidy/test/feat boundary
is already the commit structure: a `test:` commit
(`tests/corpus.rs`, 266 lines, fails to compile alone) followed by a `feat:`
commit (`src/corpus.rs` + the one-line `lib.rs` module registration, ~520
lines). Splitting into two *sub-PRs* — the unit this budget actually
measures — is not available to a single advance under this repo's
phase/sub-PR discipline, and would not help here regardless: the tests exist
to prove specific claims about specific fixture bodies and golden queries
(exact leak-bait counts, exact precedence-merge scope, exact contested-pair
membership), so a test-only PR would be unreviewable on its own (nothing to
run it against) and a generator-only PR would ship 30+ fixtures and four
scenario categories with no evidence any of them behave as claimed. The size
score is dominated by the fixture data itself (36 records × explicit scope,
provenance, and tags, by design — a corpus this small could not cover six
categories × four scopes plus five extra scenarios with less data).

## Changes Made

### 2026-08-06 - test: corpus integration tests (fail without the generator)
- crates/totem-store/tests/corpus.rs: new — 9 tests proving full category ×
  scope coverage, deterministic reseeding (`seeded_in_memory`), working
  golden queries (top-1 and membership), near-scope precedence collapsing to
  the narrower scope, contested-pair visibility, aged/expired fixture
  presence, and leak-bait isolation (count and provenance, not just body
  content) — committed before `src/corpus.rs` existed, so this commit alone
  does not compile

### 2026-08-06 - feat: synthetic memory corpus generator
- crates/totem-store/src/corpus.rs: new — the 6×4 category × scope-tier grid
  (24 fixtures), two leak-bait pairs (Knowledge + Identity, byte-identical
  content at `corpus-nova`'s and `corpus-juniper`'s private actor scopes), a
  near-duplicate Knowledge pair, an aged Knowledge fixture and an expired
  Context fixture, a contested Uncertainty pair, and a precedence pair (same
  body at actor and project scope, proving the store's narrowest-scope-wins
  merge); `GoldenQuery`/`LeakBaitPair` data types, `golden_queries()` /
  `leak_bait_pairs()`, `run_golden_query()`, `seed()`, and
  `seeded_in_memory()`. Every record's content is generator-tagged
  (`GENERATOR_TAG`) and passed through the existing `DeterministicEmbedder`
  so vector recall in the golden queries is real cosine geometry, not a
  hand-computed distance.
- crates/totem-store/src/lib.rs: `pub mod corpus;`

### 2026-08-06 - docs: complete ADV-STORE-005
- arrive/systems/058-totem-core/advances/ADV-STORE-005.md: status complete,
  evidence, practice dispositions, two Corrections entries (library call
  instead of a CLI binary; top-1/membership golden queries instead of full
  multi-rank scoring), Reviewability justification, refreshed CFU
- arrive/implementation-plan.yaml: plan item ADV-STORE-005 set to done

## Check for Understanding

1. `run_golden_query` embeds `probe_text` with the same `DeterministicEmbedder`
   `seed` used to embed every fixture body. Why does that matter for
   `vector_recall_ranks_the_matching_instruction_first`'s `expected_top`
   guarantee, and what would break if the query used a different (but still
   deterministic) embedder instead?
2. `merge_chain` (`memory.rs`) dedupes on `(category, subject, normalized
   body)`. The precedence pair writes byte-identical `PRECEDENCE_BODY` at
   `actor:corpus-nova` and `project:corpus/rocket`. Walk through why
   `near_scope_precedence_collapses_the_precedence_pair_to_one_record` passes,
   and what would happen to that test if the two writes used slightly
   different wording instead.
3. `NEAR_DUP_A` and `NEAR_DUP_B` are two different Knowledge records at the
   same project scope, deliberately worded differently. Given `merge_chain`'s
   exact-normalized-body dedup key, why do they *not* collapse into one record
   the way the precedence pair does — and what does that imply about what
   "near-duplicate" fixtures are actually testing versus what the leak-bait
   fixtures test?
4. Both leak-bait pairs write the exact same body to `corpus-nova`'s and
   `corpus-juniper`'s private actor scopes.
   `leak_bait_pairs_never_cross_the_private_scope_boundary` asserts a record
   count of exactly 1 *and* a specific `provenance.author`, not just that the
   body is present. Why is a body-presence check alone insufficient to catch
   a real scope leak in this fixture design?
5. `seeded_in_memory` is this advance's answer to "reset," but it does not
   call `DELETE`. Where in `schema.rs` is that refusal enforced, and why does
   an Episodic fixture in the corpus make in-place deletion structurally
   impossible rather than just inadvisable?
6. `fixture()` originally took 9 positional arguments and failed
   `clippy::too_many_arguments`. It now takes a `Provenance` in place of
   `author`/`harness`/`session`/`created_at`. Does that change lose any
   information the four separate parameters carried, or does it only change
   where that information is assembled?
7. The frontmatter records `tdd` as `not_applicable` even though
   `tests/corpus.rs` was committed in a commit before `src/corpus.rs` existed
   and would not compile alone. Reconcile those two facts using
   `arrive-advance-profiles.md`'s work-product rule.
8. The Outcome section promises "one command seeds a reproducible corpus,"
   but no CLI binary was built — see the first Corrections entry. Given who
   the declared immediate consumers are (ADV-CORE-005, ADV-GATEWAY-006,
   ADV-GATEWAY-008), was that the right place to stop scope, or does it leave
   a real gap for one of them?
