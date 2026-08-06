---
advance:
  id: "ADV-GATEWAY-004"
  title: "MCP tools: feedback, contest, advance_status/advance_log (gap-fill)"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T11:49:51Z"
  review_time_estimate_minutes: 35
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 91
  risk_flags: ["public_api", "migration", "concurrency"]
  evidence: ["tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

**Gap-fill** (see docs/arrive-decomposition-gaps.md): four of the seven MCP
tools in Solution Intent §3.1 are not covered by any roadmap advance. Add
`totem_feedback` (explicit value signal: used / wrong / stale — the input side
of the value loop), `totem_contest` (file an Uncertainty record instead of
overwriting), and `totem_advance_status` / `totem_advance_log`
(process-attuned read/write for the active advance).

**Correction to the original objective (2026-08-06):** `components` was
authored as `["gateway", "core"]`, omitting `store`. Implementing
`totem_feedback` honestly (an explicit signal has to persist somewhere, not
just travel through the gateway) needed a new `MemoryRepository::apply_feedback`
method, and `totem_advance_status` needed a new
`LandscapeRepository::advance` lookup — both store-layer additions. Corrected
above rather than silently narrowing `totem_feedback` to a no-op signal.

## Behavioral Change

After this advance:
- Agents can signal memory value explicitly (`totem_feedback`: `used` raises
  `value_score`, `wrong` lowers it, `stale` resets `currency` to `0.0`) —
  alongside ADV-CORE-002's automatic citation-boost and usage-reinforcement
  signals, not replacing them.
- A contradiction becomes an Uncertainty record with both claims preserved
  (`totem_contest`): the original memory is never revised, and the new claim
  is a separate record subject-linked back to it — agents no longer have to
  overwrite or silently drop conflicting facts.
- An agent working an advance can append a process-attuned log entry
  (`totem_advance_log`, an Episodic record subject-linked to the advance) and
  read the advance's current status from the landscape mirror
  (`totem_advance_status`).
- All four are available over both MCP (stdio) and REST, backed by the same
  `ops.rs` functions `totem_recall`/`totem_save` already share with their own
  REST twins, so the two surfaces cannot diverge.

## Component Impact

`components: ["gateway", "core", "store"]`, `primary_component: "gateway"`
(see the Objective correction above for why `store` was added).

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change) — none needed; every
      change is additive to existing modules.
- [x] test: tool tests incl. Uncertainty-record filing semantics
- [x] feat: four MCP tools + backing REST endpoints

## Bug Fixes

- None.

## Risk + Rollback

- Risk: `totem_advance_log` writes about ARRIVE artifacts — writes go to
  Totem's mirror/memory only; `/arrive/` files in the repo stay authoritative
  (arrive-sync invariant). Verified by
  `advance_log_writes_an_episodic_record_linked_to_the_advance` (REST) and
  `totem_advance_log_then_totem_advance_status_round_trips_over_stdio` (MCP):
  neither test touches `/arrive/`, only the store.
- Risk: `totem_feedback` and `totem_contest` both act on an existing memory
  named only by id — a caller who cannot see that id must not learn it
  exists. `apply_feedback` reads via the writer's own chain first (the same
  refusal `revise` already makes) and `contest` explicitly checks visibility
  before filing, both proven not-found rather than forbidden by
  `feedback_on_a_record_outside_the_callers_chain_reads_as_not_found`,
  `feedback_on_a_memory_outside_the_callers_chain_is_not_found`, and
  `contesting_an_id_outside_the_writers_chain_is_not_found`.
- Risk: migration 4 widens `access_log`'s `operation` field assertion in
  place (`DEFINE FIELD OVERWRITE`) rather than adding a new field. Proven
  additive and non-breaking by the full existing `schema_contracts.rs` suite
  (including `migrations_apply_once_and_replay_as_a_no_op`) staying green
  against the widened assertion, plus `apply_feedback`'s own tests exercising
  the new `feedback` operation value end-to-end.
- Rollback: revert this sub-PR's merge commit; every change is additive
  (a new module, new repository methods, new routes/tools) — nothing here
  alters `totem_recall`/`totem_save`/`totem_landscape`'s existing behavior or
  wire shape, so a revert cannot regress them.

## Reviewability

`arrive score` reports **91 [RED]** (Size 68, Novelty 8, Risk 15; flags
`migration`, `concurrency`). Not split, for two reasons:

1. **The process gate.** This run implements exactly one advance per the
   phase-per-run protocol (`docs/cloud-agent-notes.md`) — the sub-PR is the
   unit of review, and this advance's own scope (four tools, Objective above)
   was fixed before this run started, not decided here.
2. **The four tools do not decompose cleanly.** Each pairs one MCP tool with
   one REST handler routed through one shared `ops.rs` function — the same
   architecture `totem_recall`/`totem_save` already establish, so splitting
   by tool would still touch `dto.rs`, `ops.rs`, `handlers.rs`, `mcp.rs`, and
   `lib.rs` four times over, in near-identical shape, without reducing what a
   reviewer has to understand in any one diff.

What the size instead comes from: `totem_feedback` and `totem_advance_status`
needed real `store`-layer additions (`MemoryRepository::apply_feedback`,
`LandscapeRepository::advance`, a schema migration), not just gateway
plumbing — the corrected `components` list above reflects that this was a
three-layer advance, not a one-layer one.

Mitigating the size for review: the sub-branch's three `feat:` commits are
split by architectural layer (`core`, then `store`, then `gateway`), each
independently compiling and passing its own tests — a reviewer can review and
verify layer by layer rather than as one diff.

**TDD note, honestly:** tests were **not** written strictly red-first commit
by commit — core, store, and gateway were implemented and tested together
within each layer, then verified green as a whole (`cargo test --workspace`).
`evidence` therefore omits `tdd:red-green`; it is not claimed. Every test
listed under Risk + Rollback above is real and passing, verified by running
`cargo test --workspace` after implementation, not asserted from memory.

## Evidence

- [x] tests:unit — `crates/totem-core/src/feedback.rs`'s own `#[cfg(test)]`
  module (5 tests: `used`/`wrong`/`stale` behavior, the zero-floor on
  repeated `wrong` signals, JSON round-trip).
- [x] tests:integration — `crates/totem-store/tests/feedback.rs` (6 tests),
  `crates/totem-store/tests/landscape_sync.rs`'s two new `advance()` tests,
  `crates/totem-gateway/tests/feedback_contest_advance.rs` (9 REST tests),
  `crates/totem-gateway/tests/mcp_feedback_contest_advance.rs` (5 real stdio
  MCP round-trip tests, spawning the actual `mcp_stdio` binary).
- [ ] tdd:red-green — not claimed; see the Reviewability section's TDD note.
- [ ] tidy:preparatory — not applicable; nothing needed tidying first.

## CI Evidence Notes

- CI was not observed for this push (no run was read back in this session).
  Checks run externally before this sub-PR opened:
  - `cargo fmt --check` — clean.
  - `cargo clippy --workspace --all-targets -- -D warnings` — clean.
  - `cargo test --workspace` — all green (core, store, gateway, arrive-sync,
    cli, console, embedding-spike, mcp-spike, store-spike; doctests
    included).
  - `arrive doctor artifacts`, `arrive plan check`, `arrive check --strict`,
    `arrive score` — run as part of Step 7; see the journey report for exact
    output.

## Changes Made

### 2026-08-06 - feat: core: `FeedbackSignal` domain type
- `crates/totem-core/src/feedback.rs`: new module — `FeedbackSignal`
  (`Used`/`Wrong`/`Stale`), `apply_feedback` (pure function over
  `Economics`), `USED_BOOST`/`WRONG_PENALTY` constants, 5 unit tests.
- `crates/totem-core/src/lib.rs`: registers and re-exports the new module.
- `crates/totem-core/src/access_log.rs`: adds `AccessOperation::Feedback`.

### 2026-08-06 - feat: store: `apply_feedback` and advance-by-id lookup
- `crates/totem-store/src/memory.rs`: adds
  `MemoryRepository::apply_feedback` — refuses append-only categories and
  records outside the writer's chain, same as `revise`.
- `crates/totem-store/src/landscape.rs`: adds `LandscapeRepository::advance`
  — one advance by id, no repo qualifier needed.
- `crates/totem-store/src/access_log.rs`: maps `AccessOperation::Feedback`
  to/from the stored `"feedback"` key.
- `crates/totem-store/src/schema.rs`: migration 4 —
  `DEFINE FIELD OVERWRITE operation ON access_log` widens the `ASSERT` set to
  admit `'feedback'`.
- `crates/totem-store/src/migrate.rs`: registers migration 4.
- `crates/totem-store/tests/feedback.rs`: new — 6 tests for
  `apply_feedback`.
- `crates/totem-store/tests/landscape_sync.rs`: 2 new tests for
  `LandscapeRepository::advance`.

### 2026-08-06 - feat: gateway: feedback/contest/advance tools (gap-fill)
- `crates/totem-gateway/src/ops.rs`: adds `FeedbackInput`/`feedback`,
  `ContestInput`/`contest` (delegates to `save` after a visibility check),
  `AdvanceLogInput`/`advance_log` (delegates to `save`), and
  `advance_status`.
- `crates/totem-gateway/src/dto.rs`: adds the REST request/response shapes
  for all four.
- `crates/totem-gateway/src/handlers.rs`: adds the four REST handlers.
- `crates/totem-gateway/src/mcp.rs`: adds `totem_feedback`, `totem_contest`,
  `totem_advance_log`, `totem_advance_status` tools, their params structs,
  and parsing functions.
- `crates/totem-gateway/src/error.rs`: adds `GatewayError::InvalidRequest`
  (400) for the one caller-facing rule this layer enforces itself (a
  non-empty `advance_id`).
- `crates/totem-gateway/src/lib.rs`: routes `POST /feedback`,
  `POST /contest`, `POST /advance/log`, `GET /advance/:id/status`; exports
  the new DTOs.
- `crates/totem-gateway/tests/feedback_contest_advance.rs`: new — 9 REST
  integration tests.
- `crates/totem-gateway/tests/mcp_feedback_contest_advance.rs`: new — 5 real
  stdio MCP integration tests.

## Check for Understanding

1. `totem_feedback`'s `wrong` signal lowers `value_score` by a flat
   `WRONG_PENALTY` (0.3), floored at `0.0`
   (`crates/totem-core/src/feedback.rs`). Why floor at zero instead of
   letting repeated negative feedback drive it negative, and what would break
   in `totem-store/src/memory.rs`'s `rank_score` if it didn't?
2. `totem_contest` (`crates/totem-gateway/src/ops.rs::contest`) checks that
   the contested memory is visible to the writer's chain *before* filing the
   Uncertainty record, and returns the same `NotFound` a caller gets for
   reading an invisible id directly. What information would leak if it
   skipped that check and let the store's own scope check on the *new*
   record's write be the only gate?
3. `totem_advance_log` and `totem_contest` both delegate to the existing
   `ops::save` rather than adding their own store-writing code
   (`crates/totem-gateway/src/ops.rs`). What two things do they get "for
   free" from that reuse, and what field of `SaveInput` does each set that a
   plain `/save` call leaves to the caller's choice?
4. `totem_advance_status` (`crates/totem-store/src/landscape.rs::advance`)
   does not take a repo id, unlike `LandscapeRepository::view`. What makes
   that safe for advances specifically, and why would the same shortcut be
   wrong for, say, looking up a component by its short id alone?
5. Migration 4 (`crates/totem-store/src/schema.rs`) uses
   `DEFINE FIELD OVERWRITE` instead of a plain `DEFINE FIELD`. What happens
   if you replace it with an unadorned `DEFINE FIELD operation ON access_log
   ...` against a store that already ran migrations 1–3, and which test in
   `crates/totem-store/tests/feedback.rs` would first show the failure?
