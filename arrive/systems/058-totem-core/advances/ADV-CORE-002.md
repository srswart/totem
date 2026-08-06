---
advance:
  id: "ADV-CORE-002"
  title: "Metering + value/currency scoring in retrieval ranking"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T10:51:37Z"
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 59
  risk_flags: []
  evidence: ["tidy:preparatory", "tdd:red-green", "tests:unit", "tests:integration"]
  practices:
    tidy_first:
      status: applied
    tdd:
      status: applied
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

Implement the value loop (Solution Intent §4): per-memory counters
(retrievals, injections, citations), per-actor/per-harness usage aggregates,
`value_score` updated from explicit feedback and implicit use signals, and a
`currency` term that decays with time and refreshes on reinforcement.
Retrieval ranking becomes relevance (vector + graph proximity) × value ×
currency, weighted per category.

**Scope narrowed against the v1 signal set ADV-CORE-004 actually recommends**
(docs/tech-direction/value-attribution.md): the Objective above, written
before that investigation ran, names three inputs — explicit feedback,
per-actor/per-harness aggregates, and a separate "injections" counter — that
the investigation's own findings say are either not yet buildable or not yet
worth building. See Behavioral Change for exactly what landed instead and
why; this is a narrowing recorded here, not a silent one.

## Behavioral Change

After this advance:
- `MemoryRepository::recall` ranks the merged view by combined score —
  relevance × value × currency, weighted per category — instead of by the
  statement's own vector-rank/recency order; `totem-core::scoring` holds the
  pure math (`decay_currency`, `effective_currency`, `category_weight`,
  `relevance_from_distance`, `combined_score`).
- Every record a recall actually returns counts as a use: `use_count`
  increments, `last_used_at` moves to now, `currency` refreshes to full trust
  — for every category except Episodic, which the schema refuses to `UPDATE`
  at all (append-only) and which `category.rs`'s own lifecycle already says
  should never be value-ranked.
- `currency` decays only for categories whose lifecycle says they decay
  (Knowledge, Context — `category.rs`'s `decays: true`); Identity,
  Instructions, Uncertainty, and Episodic hold their stored currency exactly,
  matching their own `decays: false`. Decay is a read-time computation from
  elapsed time, not a scheduled write — no process persists decay for a
  record that is never reread (that gap is ADV-CURATOR-002, not yet built;
  docs/arrive-decomposition-gaps.md already names it).
- `MemoryRepository::save` raises a cited source's `value_score` when the new
  record names it in `provenance.derived_from` (VAL-002's primary signal,
  precision 0.80 on ADV-CORE-004's labeled corpus) — scope-checked against
  the *writer's own chain* and episodic-exempt, so a citation can never cross
  an isolation boundary or reach an append-only row.
- **Not implemented, and said so rather than silently narrowed:**
  - **Explicit feedback** (`used`/`wrong`/`stale` moving `value_score`) —
    VAL-005 found zero data points to design against: `totem_feedback` is
    `ADV-GATEWAY-004`, still `planned`. `value_score` in v1 only ever moves
    upward, from citation; there is no negative signal yet.
  - **Per-actor/per-harness usage aggregates** — only per-memory counters
    exist (`Economics.use_count`/`last_used_at`). Aggregation across actors
    or harnesses is a query over `access_log` (already recording every
    recall/save with actor and harness — ADV-GATEWAY-001), not a new field;
    unclaimed here.
  - **A distinct "injections" counter** — `docs/solution-intent.md` §4 names
    it alongside retrievals/citations, but nothing today distinguishes "the
    gateway returned this record from recall" from "an agent actually placed
    it in context." Only the former exists (`use_count`, on every returned
    record); the latter needs a harness-side signal no advance currently
    owns.
  - **Gateway/MCP exposure of `derived_from` on save** — VAL-004's own gap.
    `SaveRequest` (`totem-gateway/src/dto.rs`) and `totem_save`'s MCP
    parameters (`totem-gateway/src/mcp.rs`) still take no `derived_from`
    field, so no request over REST or MCP can trigger a citation today — only
    a direct `totem-store`/`totem-core` caller (this advance's own tests, or
    a future curator) can. Citation is real at the store layer and inert at
    every surface an agent actually calls. Closing it is `gateway`-component
    work, out of this advance's declared `["core", "store"]` scope.
  - **The console "retire?" queue** surfacing low-value/low-currency memories
    — `ADV-CONSOLE-002` territory; this advance only makes the ranking signal
    exist, not the queue that reads it.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change) — threaded
      `knn_distance` through `merge_chain` without changing `recall`'s output
      (verified: pre-existing store tests unchanged before any scoring logic
      landed)
- [x] test: scoring/decay/ranking property tests (red first)
- [x] feat: counters, score updates, ranking function wired into recall

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: value attribution is the brief's acknowledged hard problem — start
  with simple proxies (retrieval → citation → outcome) and iterate; a bad
  ranking function degrades recall quality for every harness. Mitigated by
  landing only the one signal ADV-CORE-004 measured with real discriminating
  power (citation) and explicitly not weighting the one it measured as
  uninformative (raw retrieval, VAL-003).
- Risk: `DEFAULT_CURRENCY_HALF_LIFE` (14 days) and `CITATION_BOOST` (0.2) are
  placeholders, not measured values — no production recall/reinforcement
  telemetry exists yet (VAL-004/VAL-005). Both are named constants in one
  place each (`totem-core::scoring`, `totem-store::memory`) specifically so
  they are easy to retune once real usage data exists.
- Risk: citation is currently unreachable from any real agent call (see
  Behavioral Change's "not implemented" list, VAL-004) — the mechanism is
  proven at the store layer by this advance's own tests, not by live traffic.
- Rollback: feature-flag the ranking multiplier back to pure relevance;
  counters are additive data. Concretely: `rank_score` in
  `totem-store/src/memory.rs` is the one call site that combines the four
  factors — reverting it to `relevance_from_distance(distance)` alone
  restores the pre-advance ordering without touching `reinforce_usage` or
  `save`'s citation transaction, which only ever write additive/
  idempotent-shaped data.

## Evidence

- [x] tidy:preparatory — `tidy: [ADV-CORE-002] thread knn_distance through
      merge_chain`, verified behavior-preserving by running the full
      pre-existing `totem-store` suite immediately after (all green) before
      any scoring logic was added.
- [x] tdd:red-green — `totem-core::scoring`: bodies were `unimplemented!()`
      behind the full test module; `cargo test -p totem-core --lib scoring`
      showed all 16 panicking for the right reason (naming this advance, not
      a compile error or a typo) before real bodies replaced the stubs, then
      all 16 green. `totem-store/tests/value_scoring.rs`: 3 of 7 assertions
      genuinely failed against the pre-feat store (citation boost, use-count
      reinforcement, cited-record ranking) before the `feat:` commit, then
      all 7 green after. (The other 4 already held pre-feat — 3 trivially,
      since nothing touched economics yet; 1, `...ranks_a_recently_
      reinforced_decaying_record_above_a_stale_one`, also held under the
      pre-existing plain-recency ordering, so it is not a strict red/green
      case for that specific test — recorded honestly rather than claimed as
      one.)
- [x] tests:unit — 16 new tests in `totem-core::scoring` (decay monotonicity
      and boundedness, category-lifecycle-gated decay, category-weight
      ordering, relevance-from-distance monotonicity, combined-score
      algebra).
- [x] tests:integration — 7 new tests in
      `totem-store/tests/value_scoring.rs`: reinforcement counters, the
      episodic append-only guard (both for recall's reinforcement and save's
      citation path), citation boosting, a citation's scope-isolation
      boundary (an adversarial cross-scope citation claim does not move the
      cited record), and ranking order responding to both citation and
      currency decay. Every pre-existing `totem-store` test (57 across 8
      files) still passes unchanged.

## CI Evidence Notes

- CI pipeline result not observed for this push (no access to this branch's
  Actions run from this sandbox); ran the equivalent checks externally
  instead, all green: `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace`, `arrive doctor
  artifacts`, `arrive plan check`.
- `arrive pr check --strict --json` and `arrive evidence record` were not run
  (no authenticated `arrive` commands in this sandbox per
  docs/cloud-agent-notes.md Step 2) — a human or a later CI run should
  execute these before merge.

## Reviewability

`arrive score --base origin/advance/phase-006` reports **59 [YELLOW]** (size
53, novelty 6, risk 0) after the `fix:` commit addressing Copilot's PR review
(the score was 47 before it — folding `record_citations` into `save`'s own
transaction touched more of the same function than a separate method would
have). Still under the Red threshold. Not split further: the three touched
files — the pure scoring math, its wiring into `save`/`recall`, and the tests
proving both — are one cohesive change (a ranking function has no meaning
reviewed apart from the store logic that calls it, and neither has meaning
apart from the tests that pin its behavior), and the commit series already
separates tidy → test → feat → fix, so a reviewer can approve each phase
independently within the one sub-PR.

## Changes Made

### 2026-08-06 - tidy: thread knn_distance through merge_chain
- crates/totem-store/src/memory.rs: `merge_chain` now carries the vector
  distance alongside each candidate record instead of discarding it;
  `recall`'s own behavior (output records, order, limit) is unchanged —
  verified by the full pre-existing store suite passing before any scoring
  logic was added.

### 2026-08-06 - test: scoring/decay/ranking tests (red first)
- crates/totem-core/src/scoring.rs: added — `decay_currency`,
  `effective_currency`, `category_weight`, `relevance_from_distance`,
  `combined_score`, all `unimplemented!()`, plus 16 unit tests. Confirmed
  failing for the right reason before implementation.
- crates/totem-core/src/lib.rs: declared `mod scoring;` and re-exported its
  public functions/constant.
- crates/totem-store/tests/value_scoring.rs: added — 7 integration tests
  against the (pre-feat) store; 3 confirmed genuinely failing.

### 2026-08-06 - feat: metering, value/currency scoring wired into recall
- crates/totem-core/src/scoring.rs: real bodies for all five functions —
  exponential half-life decay clamped to `[0, 1]`, gated by
  `MemoryCategory::lifecycle().decays`; category weight normalized from
  `injection_priority`; vector distance to a `(0, 1]` relevance term (neutral
  `1.0` with no probe); the four-factor product. 16/16 tests green.
- crates/totem-store/src/memory.rs: `save` now boosts each cited
  (non-episodic, in-chain) source's `value_score` by `CITATION_BOOST` (0.2)
  when `provenance.derived_from` is non-empty. `recall` now scores the
  merged view with `rank_score` (relevance from `knn_distance` × `value_score`
  × live-decayed `currency` × `category_weight`), sorts, truncates to the
  caller's limit, then calls `reinforce_usage` on exactly what it returns
  (`use_count` +1, `last_used_at` to now, `currency` reset to 1.0),
  excluding episodic records twice over (Rust-side `is_append_only` filter
  and a server-side `category != $episodic` predicate).
- crates/totem-store/tests/value_scoring.rs: 7/7 green.
- arrive/implementation-plan.yaml: `ADV-CORE-002` set to `done`.
- arrive/systems/058-totem-core/advances/ADV-CORE-002.md: this record —
  status `complete`, evidence, practice dispositions, reviewability score,
  refreshed Behavioral Change/Risk/CFU.

### 2026-08-06 - fix: address Copilot PR review comments (#30)
- crates/totem-store/src/memory.rs: `save` now commits the `INSERT` and the
  citation `value_score` boost as one `BEGIN`/`COMMIT TRANSACTION` (TD-006)
  instead of two separate writes — a transient failure in the citation
  update could otherwise leave the new record inserted but return an error
  to the caller, risking a duplicate insert on retry. The standalone
  `record_citations` method (its only caller) is folded directly into
  `save`; the no-citations path is still a plain `INSERT`, no transaction
  overhead when `derived_from` is empty.
- crates/totem-store/tests/value_scoring.rs: renamed
  `recall_ranks_a_recently_reinforced_decaying_record_above_a_stale_one` to
  `...recently_written...` and corrected its comment — the test never
  performs a reinforcing recall on the "fresh" record, it only writes it
  more recently, so "reinforced" misnamed what the test actually exercises
  (currency decay from elapsed time since creation).
- Reviewability rose from 47 to 59 (still YELLOW) — recorded in this file's
  own Reviewability section.

## Check for Understanding

1. `effective_currency` (`crates/totem-core/src/scoring.rs`) checks
   `category.lifecycle().decays` before ever calling `decay_currency`. Which
   four categories does this exempt, and what would go wrong for
   `Instructions` specifically if that check were removed?
2. `reinforce_usage` and `save`'s citation update
   (`crates/totem-store/src/memory.rs`) both filter out episodic records
   *and* add a `category != $episodic` predicate to the statement itself.
   Why is the Rust-side filter alone not enough — what's the actual failure
   mode if it were?
3. `citing_a_memory_outside_the_writers_chain_does_not_boost_it`
   (`crates/totem-store/tests/value_scoring.rs`) has ada name grace's private
   memory in `derived_from` for a record ada legitimately writes to her own
   scope. Why does `save`'s citation update still refuse to move grace's
   `value_score`, given that ada's own write succeeds?
4. `save` wraps the insert and the citation boost in one
   `BEGIN`/`COMMIT TRANSACTION` only when `derived_from` is non-empty. Why
   is a single-statement `INSERT` (no transaction wrapper) still correct
   for the empty case, rather than always wrapping both statements?
5. `rank_score` computes `elapsed` from `last_used_at.unwrap_or(created_at)`
   — trace what happens to a Knowledge record's ranking position across two
   consecutive `recall` calls with nothing else changing in between, and
   explain why.
6. The Behavioral Change section lists five things this advance's Objective
   implied but does not implement (explicit feedback, per-actor/per-harness
   aggregates, an injections counter, gateway exposure of `derived_from`, the
   console retire queue). Pick the one you think is riskiest to leave
   unimplemented and say what could go wrong for a real harness before it
   lands.
7. `CITATION_BOOST` (0.2) and `DEFAULT_CURRENCY_HALF_LIFE` (14 days) are both
   named constants rather than configuration. What evidence would justify
   changing either value, and where would that evidence come from given
   VAL-004/VAL-005's findings?
