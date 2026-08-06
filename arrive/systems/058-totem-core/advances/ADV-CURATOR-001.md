---
advance:
  id: "ADV-CURATOR-001"
  title: "First curator job (dedupe) with supersede/rollback"
  system: "058-totem-core"
  primary_component: "curator"
  components: ["curator", "core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T16:45:00Z"
  review_time_estimate_minutes: 190
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 104
  risk_flags: ["concurrency", "migration"]
  evidence: ["tdd:red-green", "tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

First AI curation job: deduplication of near-duplicate Knowledge memories,
running as a background agent against the same core API (Solution Intent §5).
Establishes the curator framework and its non-negotiable action model:
supersede + log + reversible — never delete.

## Behavioral Change

After this advance:
- The dedupe job identifies near-duplicate Knowledge memories (vector
  similarity + graph context) and merges them by writing a superseding record;
  originals remain, marked superseded, with lineage links.
- Every curator action is logged and reversible: a rollback restores the
  superseded originals and retires the merge.
- The job runs idempotently and safely alongside live writes.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change) — nothing to tidy;
      the two helpers the new module needed (`row::status_key`/`status_from`,
      `memory::check_dimensions`) were already extracted, and only their
      visibility changed, which is part of the feature commit rather than a
      behaviour-preserving refactor of its own
- [x] test: supersede/rollback invariant tests — no destructive path exists (red first)
- [x] feat: curator job runner + dedupe job

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: "curation trust" is a named key risk — silent rewriting would destroy
  auditability. Mitigated by the append-only episodic substrate and the
  supersede-only action model; concurrency with live writes needs care.
- Rollback: pause the curator; run the built-in rollback for any bad merges.

## Security Notes

The curator is the first writer in Totem that touches records it did not
author, so each guard is listed with the test that proves it.

| Guard | Where | Proven by |
|---|---|---|
| A merge never crosses a scope boundary | `CurationPolicy::merge` refuses `CrossScope`; every `UPDATE` is additionally pinned to `scope = $scope` | `core/tests/curation.rs::a_merge_never_crosses_a_scope_boundary`; `curator/tests/dedupe.rs::identical_records_at_different_scopes_are_never_merged` |
| A curator only supersedes what its own chain can read | originals are re-fetched through `MemoryRepository::get`, so an invisible record reads as absent | `store/tests/curation.rs::a_curator_cannot_supersede_a_record_it_cannot_see` |
| A curator only writes the survivor where it could already write | `curator.contains(&merged.scope)` → `ScopeDenied` | `store/tests/curation.rs::a_curator_cannot_write_the_survivor_into_a_scope_it_cannot_reach` |
| Nothing is ever deleted | there is no `DELETE` statement in `store/src/curation.rs`; originals move to `Retired` and stay readable | `store/tests/curation.rs::a_merge_retires_the_originals_and_leaves_them_readable` |
| A merge applies to exactly the rows its event names, or to none | each `UPDATE` is pinned to id + scope + status, with `IF array::len(...) != n THROW` inside the transaction | `store/src/curation.rs::tests::a_merge_whose_recorded_status_is_stale_applies_to_nothing`; `store/tests/curation.rs::the_same_merge_cannot_be_applied_twice` |
| A rollback restores the status each record actually held | `Supersession::prior_status` travels in the event and is what the restoring `UPDATE` sets | `core/tests/curation.rs::a_merge_records_every_original_it_supersedes_with_the_status_it_had` |
| Human-gated categories stay human-gated | `may_curate` reads the category's own `ReviewPolicy` | `core/tests/curation.rs::only_categories_a_curator_may_act_on_alone_can_be_merged`; `store/tests/curation.rs::a_scan_refuses_a_category_no_curator_may_act_on_alone` |
| Every curator access is logged | the runner logs scan, merge, and rollback; the job never reaches the store except through it | `curator/tests/dedupe.rs::every_curator_action_appends_to_the_access_log` |
| A background scan cannot manufacture value signal | `candidates` is a separate read path that does not meter usage, unlike `recall` | `store/tests/curation.rs::scanning_for_candidates_reads_the_active_set_without_metering_it` |
| The curation trail cannot be rewritten or removed | migration 6 defines `EVENT`s refusing `UPDATE`/`DELETE` on `curation_event` | `store/src/schema.rs::tests::the_database_refuses_to_{update,delete}_a_curation_event` |

## Corrections to the Advance

None. The Objective and Behavioral Change were implementable as written.

## What This Advance Establishes Only Partially

Stated plainly rather than left for a reviewer to discover:

- **"Reversible from the console"** (`components/curator.yaml`) is reversible
  *from the API*: `CurationRepository::rollback` and `Curator::rollback` exist
  and are tested, and `CurationRepository::events`/`history` are the read
  models a console audit view needs — but no console surface renders them.
  That is ADV-CONSOLE-002's work ("audit trails, Uncertainty queue"), and this
  advance deliberately does not pre-empt it.
- **The access log has no `curate` operation.** A curator scan is logged as
  `Recall` and a merge as `Save`, distinguished only by endpoint
  (`/curator/dedupe/*`). Adding an `AccessOperation::Curate` variant is a core
  change plus a migration; ADV-CORE-006 is already the advance that widens
  this vocabulary, and it is the right place for it.
- **The similarity threshold (0.95) is not a measured value.** The corpus that
  could justify one is ADV-STORE-005, and the evaluation that would grade it
  ADV-CORE-005. It is a deliberately strict default and a documented knob.
- **No embedder runs inside the job.** Records without an embedding are
  skipped and counted in the report (`skipped_without_embedding`) rather than
  compared by text, so the job is a no-op on an unembedded corpus — visibly,
  by design, not silently.
- **The scan is unbounded.** `candidates` reads every active record of the
  category in the reader's chain. Fine at present corpus sizes and honest
  about it; batching belongs with the performance evaluation
  (ADV-GATEWAY-007) that can measure whether it matters.
- **A refused merge ends the run.** If a live write changes a record between
  the scan and the merge, the transaction refuses and `dedupe` returns the
  error; merges already recorded stand, and because the job is idempotent the
  next run re-scans. It does not skip-and-continue, which would mean deciding
  in the job which store refusals are benign.

## Reviewability

`arrive score` reports **104 [RED]** (size 69, novelty 20, risk 15; risk flags
`migration`, `concurrency`). Documented as atomic rather than split.

Of ~2,730 added lines, roughly 1,200 are tests: three new test files
(`core/tests/curation.rs`, `store/tests/curation.rs`,
`curator/tests/dedupe.rs`), an in-crate transaction-guard test, and three new
schema assertions. The production surface a reviewer must actually reason
about is `crates/totem-core/src/curation.rs` (279 lines, a third of it doc
comments), `crates/totem-store/src/curation.rs` (~500 lines before its test
module), and `crates/totem-curator/` (~420 lines).

Splitting at the component boundary — policy in one sub-PR, store enforcement
in the next, the job last — would put a `CurationPolicy` that nothing obeys on
the phase branch, followed by a supersede API with no caller. For the advance
whose entire safety argument is "the only action is reversible", an interval
where the action exists and the reversal is not yet wired is the wrong artifact
to hand a reviewer. The plan also carries this as one item, so a split would
need a new planned advance — a human scoping decision, not an agent's.

The `## Security Notes` table above is offered as the reading order: each
guard, and the test that proves it.

## Evidence

- [ ] tidy:preparatory — nothing to tidy (see Planned Implementation Tasks)
- [x] tdd:red-green — commit `345c934` is red (core/store/curator tests fail to
      compile; the three schema tests fail because a schemaless
      `curation_event` accepts the UPDATE and DELETE it must refuse), commit
      `45ad7f9` is green
- [x] tests:unit — `totem-core` curation policy tests; the in-crate
      transaction-guard test in `totem-store/src/curation.rs`
- [x] tests:integration — `totem-store/tests/curation.rs` (11 tests),
      `totem-curator/tests/dedupe.rs` (9 tests), both against the embedded
      `kv-mem` engine
- [ ] ci:passed — not claimed. CI runs on `advance/**` pushes; this record was
      written before the push, so no pipeline result existed to read.

Locally, all of `cargo fmt --check`, `cargo clippy --workspace --all-targets
-- -D warnings`, and `cargo test --workspace` pass.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CURATOR-001 --status passed`

## Changes Made

### 2026-08-06 - test: supersede/rollback invariants, red before green

- `crates/totem-core/tests/curation.rs`: new — the domain half of "curation
  never deletes": scope, category, group size, self-supersede, retired-already,
  and what a rollback carries.
- `crates/totem-store/tests/curation.rs`: new — supersede/restore against the
  embedded engine, the two scope refusals, double-merge and double-rollback,
  trail scope-filtering, and that a scan does not meter usage.
- `crates/totem-curator/tests/dedupe.rs`: new — the job end to end: near
  duplicates merge, related records do not, scopes and subjects partition,
  a second run is a no-op, every action is logged, threshold is a real knob.
- `crates/totem-store/src/schema.rs`: three assertions that `curation_event`
  is append-only and refuses an unattributable row.
- `crates/totem-curator/Cargo.toml`, `crates/totem-curator/src/lib.rs`,
  `Cargo.toml`: the crate the tests name, as a manifest and an empty lib.

### 2026-08-06 - feat: dedupe curator with supersede and rollback

- `crates/totem-core/src/curation.rs`: new — `CurationPolicy`,
  `CurationEvent`, `Supersession`, `CurationEventKind`, `CurationError`. The
  policy is the only way to mint a merge event, so an event cannot disagree
  with the records it was built from.
- `crates/totem-core/src/ids.rs`, `lib.rs`: `CurationId`; module and re-exports.
- `crates/totem-store/src/curation.rs`: new — `CurationRepository`:
  `candidates`, `merge`, `rollback`, `events`, `history`, `event`. Status
  changes are built from one `StatusMove` shape shared by merge and rollback,
  each pinned to id + scope + status with an in-transaction count check.
- `crates/totem-store/src/schema.rs`, `migrate.rs`: migration 6 —
  `curation_event`, append-only, provenance required.
- `crates/totem-store/src/memory.rs`: `recall` excludes retired records;
  `check_dimensions` is crate-visible so the survivor's embedding is checked
  on the curator's write path too.
- `crates/totem-store/src/error.rs`: `Curation`, `CurationNotFound`,
  `CurationRolledBack`.
- `crates/totem-store/src/store.rs`, `lib.rs`, `row.rs`: `Store::curation()`
  and `curation_with_policy()`; `status_key`/`status_from` shared with the new
  module.
- `crates/totem-curator/src/lib.rs`: the runner — curator identity on every
  write, an access log entry for every scan, merge, and rollback.
- `crates/totem-curator/src/dedupe.rs`: the job — group by scope and subject,
  cluster by cosine similarity, build a survivor that keeps the newest
  wording, the union of tags, and the accumulated economics of what it
  replaces.
- `Cargo.toml`: `crates/totem-curator` joins the workspace; the reserved-slot
  comment is retired now that every §7 crate exists.

## Check for Understanding

1. `CurationPolicy::merge` is the only constructor for a merge event, and it
   takes `&MemoryRecord`s rather than ids. What would become expressible if it
   took `Vec<MemoryId>` and a `Scope` instead — and which of this advance's
   tests would still pass?
2. `CurationRepository::merge` re-reads every original through
   `MemoryRepository::get` before judging it, and then judges the *stored*
   copies. What does the caller's own copy of a record fail to tell you, and
   which test fails first if the re-read is dropped?
3. Each status change is pinned to the status the event recorded, not merely
   to the record's id. Describe the interleaving with a live write that this
   catches, and say what would be left in the store if the
   `IF array::len(...) != n THROW` were removed.
4. `Supersession` carries `prior_status`. Give the sequence of events where a
   rollback that assumed `Active` instead would change a record's meaning.
5. The dedupe job partitions candidates by scope *before* comparing
   similarity, rather than filtering cross-scope pairs afterwards. Both would
   pass `identical_records_at_different_scopes_are_never_merged` — so what is
   the argument for the partition, and where else in the stack does the same
   argument appear?
6. `candidates` exists instead of the job calling `recall`. What does `recall`
   do to a record it returns, and what would an hourly curator run have done
   to the value loop (G4) if the job had used it?
7. `recall` now excludes `Retired` but not `Contested`. Justify that split
   from the two statuses' definitions, and name a reader who needs the
   contested record.
8. A curator scan is logged as `AccessOperation::Recall` with endpoint
   `/curator/dedupe/scan`. What question can an auditor *not* answer from the
   access log today, and which planned advance closes it?
