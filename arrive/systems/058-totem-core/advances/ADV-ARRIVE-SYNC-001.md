---
advance:
  id: "ADV-ARRIVE-SYNC-001"
  title: "Ingest this repo's own /arrive/ into the landscape graph (dogfood)"
  system: "058-totem-core"
  primary_component: "arrive-sync"
  components: ["arrive-sync", "store", "gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T05:45:00Z"
  review_time_estimate_minutes: 40
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 100
  risk_flags: ["migration", "concurrency", "new_dependency"]
  evidence: ["tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

First landscape ingestion: parse this repo's own `/arrive/` artifacts
(registry, system, components, advances) into graph entities
(`repo` → `system` → `component`, `advance` with status, `impacts` /
`owned_by` edges) with sync provenance recorded (Solution Intent §2.3).

**Correction to the original objective:** `depends_on` (component →
component) is dropped from this advance's scope. The schema already declares
the edge table (ADV-STORE-001), but nothing in this repo's `/arrive/`
artifacts — `system.yaml`, `components/*.yaml`, or advance frontmatter —
declares an inter-component dependency; `implementation-plan.yaml`'s
`dependencies` field is *advance → advance*, a different relationship the
schema has no edge for. Inventing data to populate `depends_on` would be
worse than leaving it empty. It stays reserved in the schema for a future
advance that has a real source for it.

## Behavioral Change

After this advance:
- Running the sync against `058-totem` populates the landscape graph:
  `totem_landscape` answers "what is this project made of, what's in flight,
  what just finished" in one round trip (G2) — verified against this repo's
  own real `/arrive/` tree, not a synthetic fixture.
- Re-running the sync is idempotent: every entity uses a deterministic id, and
  `owned_by`/`impacts` edges are replaced wholesale on each run so a dropped
  owner or a retargeted advance does not leave a stale edge behind.
- Every sync run appends one `sync_run` provenance row (append-only, same
  enforcement pattern as `access_log`).
- `/arrive/` files remain authoritative — ingestion only reads them.
- `totem_landscape` (ADV-GATEWAY-002's stub) now queries the real landscape
  repository instead of always answering empty. A repo that has never been
  synced still answers with an empty landscape (`repo: null`) rather than an
  error — "not yet enrolled" is a normal state. This is a `gateway`-component
  change beyond the advance's original declared scope (`arrive-sync`,
  `store`); added because the stub's own doc comment named this advance as
  the trigger to remove it, and leaving `totem_landscape` permanently empty
  after ingestion exists would contradict the Behavioral Change this advance
  is claiming. `components` above is corrected to include `gateway`.
- The REST API gets no landscape endpoint — Solution Intent §3.2 names one,
  but no route or handler exists for it yet (only the MCP tool does); adding
  one is out of scope here and left for whichever advance next touches
  `handlers.rs`.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change) — none needed; this
      advance adds new modules/crates rather than refactoring existing code
- [x] test: parse + graph-mapping tests using this repo's artifacts as fixtures (red first)
- [x] feat: artifact parser, graph writer, sync provenance record

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: mapping drift between ARRIVE artifact schema versions and the graph
  model; mitigated by dogfooding on this repo first — `totem-arrive-sync`'s
  `tests/dogfood.rs` parses and syncs this repo's *real* `/arrive/` tree, so a
  schema-shape drift between the `arrive` CLI's writer and this reader fails
  a test here before it fails silently elsewhere.
- Risk: two systems each declaring a component with the same short id would
  collide if components were keyed by that id alone; mitigated by keying
  `component` records `<system>__<component>` internally (recovered via the
  new `component_id` field for reads). Cross-*repo* collision (two repos each
  registering a system id like `core`) is not mitigated — `repo`/`system`
  keys are not repo-namespaced. Acceptable for this single-repo dogfood
  advance; flagged as a real gap for `ADV-ARRIVE-SYNC-002` (continuous
  multi-repo sync, per `docs/arrive-decomposition-gaps.md`).
- Rollback: revert branch; landscape entities can be re-derived from `/arrive/`
  at any time (the mirror is disposable, the repo is authoritative). The new
  `component_id` field and `sync_run` table (migration 3) are additive — no
  existing migration or row shape is altered.

## Reviewability

`arrive score --base origin/advance/phase-005` reports **100, RED** (after
the Copilot-review fix commit; 65/20/15 breakdown, same flags). Splitting
further rather than justifying
atomicity was considered and rejected:

- **Store write path alone** (`landscape.rs`, migration 3, its tests) would
  land a repository nothing calls — reviewable in isolation, but not
  runnable or demonstrably correct against real data until the parser
  exists to feed it.
- **Parser alone** (`totem-arrive-sync` with no store write path) would land
  a crate that produces a `LandscapeSnapshot` nobody persists — the
  dogfood claim ("running the sync against 058-totem populates the
  landscape graph") would be unverifiable by a reviewer running the code.
- **Deferring the `totem_landscape` gateway wire-up** to a second advance
  was the one split seriously considered, since it is the smallest,
  lowest-risk piece (one stub replaced by one real call, ~50 lines). It
  stayed in this advance because ADV-GATEWAY-002's own stub doc comment
  names *this advance* as the trigger for its removal, and a second advance
  solely to delete a stub's dead branch is process overhead disproportionate
  to the change.

The three pieces are one behavior — "sync this repo's `/arrive/` into a
queryable landscape" — verified end-to-end in `tests/dogfood.rs`; splitting
along crate boundaries would have produced reviewable-looking diffs that
were each individually unverifiable. The `concurrency` flag is `arrive
score`'s pattern match on dependency-manifest churn (`Cargo.lock`,
`Cargo.toml` diffs across the new crate), not a concurrency-bearing code
change — no new async/threading logic was added; recorded as detected
rather than suppressed, since the score should reflect what the tool
measured.

## Evidence

- [ ] tidy:preparatory — not applicable; no preparatory refactor was needed
- [x] tdd:red-green — **partial, disclosed rather than blanket-claimed.**
      `totem-store`'s landscape module genuinely followed red-first: both
      `tests/landscape_sync.rs` and the `sync_run` append-only tests added to
      `schema.rs` were written and referenced `LandscapeRepository`/`sync_run`
      before `landscape.rs` or migration 3 existed, so they failed to compile
      until the implementation landed — the same "red is a compile error for
      a wholly new module" convention this repo's own history uses (e.g.
      ADV-GATEWAY-002's `0259e35`). `totem-arrive-sync`'s artifact parser and
      the `totem_landscape` MCP wiring in `totem-gateway` were **not**
      written test-first: the parser needed to be run against this repo's
      real, messy YAML/Markdown shapes to discover the right structs (an
      owner entry with neither `team` nor `user`, frontmatter closed by a
      line that is exactly `---`, sorted directory iteration), and the
      gateway wiring is a small, low-risk swap of a literal stub for a real
      call. Tests exist and pass for both, added alongside/after the code
      rather than before it. Not evidence of `tdd:red-green` for those two
      pieces; recorded here rather than left implicit.
- [x] tests:unit — `totem-store/src/schema.rs` (`sync_run` append-only
      enforcement, 2 tests); `totem-arrive-sync/src/lib.rs`'s frontmatter
      extractor is exercised indirectly through the integration tests below
      (no isolated unit test for it — a real advance file is a more honest
      fixture than a hand-built string, and the integration tests already
      cover its failure modes: missing directory, real advance files).
- [x] tests:integration — `totem-store/tests/landscape_sync.rs` (4 tests: a
      sync writes every entity, an unsynced repo is empty, re-sync is
      idempotent and drops stale edges, every sync appends provenance);
      `totem-arrive-sync/tests/dogfood.rs` (3 tests: parsing this repo's real
      `/arrive/` tree, syncing it end-to-end into a store and reading it back
      via `view()`, a missing directory reports plainly);
      `totem-gateway/tests/mcp_recall_and_save.rs` (2 tests, revised: an
      unsynced repo's `totem_landscape` answer is really empty rather than a
      hardcoded stub, and a missing `repo` param is a protocol-level error).

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-ARRIVE-SYNC-001 --status passed`

## Changes Made

### 2026-08-06 - test: landscape sync tests (red first)

- `crates/totem-store/tests/landscape_sync.rs`: new — sync/view/idempotency/
  provenance tests against `LandscapeRepository`, which did not exist yet.
- `crates/totem-store/src/schema.rs`: added `sync_run` append-only enforcement
  tests (`the_database_refuses_to_update_a_sync_run_row`,
  `..._delete_a_sync_run_row`), referencing a table migration 3 had not yet
  defined.
- `crates/totem-arrive-sync/tests/dogfood.rs`: new — parsing and end-to-end
  sync tests against this repo's real `/arrive/` tree (added alongside the
  parser rather than strictly before it; see Evidence).
- `crates/totem-gateway/tests/mcp_recall_and_save.rs`: revised the
  `totem_landscape` stub test into two tests matching real (not stubbed)
  behavior.

### 2026-08-06 - feat: landscape ingestion, sync, and the gateway wire-up

- `crates/totem-store/src/schema.rs`: migration 3 — `component_id` field on
  `component` (recovers the short artifact id from its system-namespaced
  record key) and the append-only `sync_run` table.
- `crates/totem-store/src/migrate.rs`: registered migration 3.
- `crates/totem-store/src/landscape.rs`: new — `LandscapeRepository` with
  `sync` (idempotent write, one transaction, TD-006), `view` (the merged G2
  read, one round trip using confirmed graph-traversal syntax per TD-001),
  and `sync_runs` (provenance list); the artifact/view domain types.
- `crates/totem-store/src/store.rs`, `src/lib.rs`: exposed `Store::landscape()`
  and the new public types.
- `crates/totem-store/Cargo.toml`: added `serde` (the view types are a read
  model the gateway serializes directly, the same way `totem-core`'s
  `MemoryRecord` already is).
- `crates/totem-arrive-sync/`: new crate — `read_repo_artifacts` (registry +
  system + components + advance-frontmatter parser) and `sync_repo` (parse +
  store write in one call). Added `serde_yaml` as a new workspace dependency.
- `Cargo.toml`: registered `crates/totem-arrive-sync` as a workspace member;
  removed its now-obsolete placeholder-slot comment line.
- `crates/totem-gateway/src/mcp.rs`: wired `totem_landscape` to
  `Store::landscape().view()`, replacing the always-empty stub; corrected
  `LandscapeParams.repo`'s doc comment (it is the ARRIVE registry id, not the
  `owner/name` scope form `totem_recall`/`totem_save` use — two different id
  spaces).

### 2026-08-06 - fix: address Copilot review (PR #20)

- `crates/totem-arrive-sync/src/lib.rs`: `sorted_entries` no longer drops a
  per-entry `read_dir` error via `.ok()` — a directory entry that fails to
  read now fails the whole ingestion instead of silently producing a
  possibly-incomplete snapshot.
- `crates/totem-arrive-sync/src/lib.rs`, `Cargo.toml`: `sync_repo` is now
  generic over `surrealdb::Connection` instead of hardcoded to the embedded
  `Db` engine — it only ever calls `Store::landscape()`, so it makes no
  assumption about which connection type the caller's store was built
  against.
- `crates/totem-store/src/landscape.rs`: `count()` (used for `sync_run`'s
  `*_synced` fields) now matches `Number::Int` directly instead of routing
  through the shared `f64`-widening `row::number` helper, so a
  wrong-shaped stored value (a float, an out-of-range integer) is reported
  as malformed rather than silently truncated.

## Check for Understanding

1. Why does `LandscapeRepository::sync` wrap every `UPSERT`/`RELATE`/`DELETE`
   and the final `sync_run` `CREATE` in one `BEGIN TRANSACTION` /
   `COMMIT TRANSACTION` (`crates/totem-store/src/landscape.rs`), rather than
   writing the provenance row in a separate call after the entities commit?
2. `component` records are keyed `<system>__<component>` internally
   (`ComponentArtifact::key`) instead of by the component's own short id.
   What problem does that solve, and what problem does it *not* solve — see
   the second Risk + Rollback bullet?
3. `strings()` in `landscape.rs` silently drops `None`/`Null` entries from a
   graph-traversal projection like `->impacts->component.component_id`
   instead of treating them as malformed rows. What real situation produces
   a `None` there, and why is dropping it the right call rather than an
   error? (See `re_syncing_is_idempotent_and_drops_stale_edges` and the
   `strings` doc comment.)
4. `totem_landscape`'s `LandscapeParams.repo` is documented as the ARRIVE
   registry id, not the `owner/name` scope form. Where in the codebase is
   `owner/name` the correct id to pass instead, and why are the two kept
   separate rather than unified into one `repo` concept?
5. The Evidence section declines to check `tdd:red-green` as a whole-advance
   claim. Which files in `Changes Made` genuinely followed red-first, which
   did not, and what does the advance body give as the reason for the
   difference?
