---
advance:
  id: "ADV-STORE-001"
  title: "SurrealDB schema + repositories + scope-isolation tests"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store", "core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-05T18:56:51Z"
  review_time_estimate_minutes: 120
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 101
  risk_flags: ["migration", "new_dependency", "concurrency"]
  evidence: ["tidy:preparatory", "tdd:red-green", "tests:unit", "tests:integration"]
  # Populated by `arrive usage import` / the LiteLLM callback — leave empty when authoring.
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

Define the SurrealDB schema for typed memory and the landscape graph, a
migration mechanism, and repository APIs in `totem-store` — with scope
isolation enforced at the store layer as an invariant (Solution Intent §2.2,
Project Brief "Key risks": leaking private context across scopes is the
highest-severity failure).

## Behavioral Change

After this advance:
- Memory records persist and load through repositories (embedded SurrealDB for
  dev/tests) with all frontmatter groups: identity, content, provenance,
  economics, governance.
- Scope-chain reads (actor → project → team → platform) resolve with dedup and
  precedence into one merged view.
- Tests prove the store invariants: a caller scoped to one actor cannot read
  another actor's memories; episodic records reject updates/deletes; writes
  without provenance are rejected.

Three properties are structural rather than procedural, which is the part worth
reviewing closely:

- **There is no unscoped read.** Every repository method takes a `ScopeChain`;
  a chain can only be built by `ScopeChain::resolve`, which names one actor's
  own private scope and nobody else's; the predicate is generated from it inside
  the store; and `Store::connection()` is `#[cfg(test)]`-only, so no caller
  outside the crate can write a statement that omits the filter.
- **A foreign record reads as absent, not forbidden.** `get` returns `Ok(None)`
  and `revise` returns `NotFound`. An error that distinguished "exists but
  denied" from "does not exist" would let a caller enumerate another actor's
  memory ids — a leak even when no body travels.
- **Append-only is the database's rule, not the repository's.** Two
  `DEFINE EVENT`s `THROW` on `UPDATE` and `DELETE` of an episodic row, so a
  curator job, a backfill, or a future statement written inside this crate is
  refused too. Provenance is enforced the same way, by the schema.

## Constraints from completed investigations (binding)

Read [docs/tech-direction/surrealdb.md](../../../../docs/tech-direction/surrealdb.md)
§4 before writing any schema — its constraints (TD-002 knn syntax with EXPLAIN
assertions, TD-004 datetime binding with a regression test, TD-005, TD-008) are
requirements of this advance, not background. Additionally:

- **Vector index pin:** `DIMENSION 384 DIST COSINE` — derived from the
  measured embedding decision in
  [docs/tech-direction/embeddings.md](../../../../docs/tech-direction/embeddings.md)
  (EMB-004: BGE-small-en-v1.5 via fastembed). The embedding *pipeline* lands in
  ADV-STORE-002; the schema fixes the dimension now so no index migration is
  needed between the two.
- **Connection identity (TD-011):** the store connects as a fully-privileged
  system user and enforces ALL authorization itself. Restricted DB roles are
  unusable — their data writes are silently discarded, and roles are not row
  filters. This is now a component invariant on `store`.

How each was honoured:

| Constraint | Where it landed |
|---|---|
| TD-002 `<|K,EF|>` + EXPLAIN assertion | `RecallQuery::statement` formats `<|K,EF|>`; `tests/schema_contracts.rs::vector_recall_uses_the_hnsw_index_rather_than_a_full_scan` asserts `KnnScan`, the index name, and the *absence* of `KnnTopK` |
| TD-003 scope predicate inside the index scan | `tests/scope_isolation.rs::the_scope_predicate_is_generated_by_the_store_and_reaches_the_index_scan`, with the foreign row seeded *nearer* the probe than anything readable |
| TD-004 datetimes are typed | `RecallQuery::since` takes `DateTime<Utc>`; `row::instant` is the single conversion point; the plan assertion checks for a `d'…'` literal |
| TD-005 no `vector::distance::cosine` | Not used; ranking is `vector::distance::knn()`, which is only meaningful in a knn statement |
| TD-008 notifications are unordered within a transaction | No live-query consumer in this advance; recorded for ADV-CONSOLE-001 |
| TD-011 privileged connection | `Store` doc comment; no DB user/role is created, and no `PERMISSIONS` clause is relied on |
| EMB-004 384 / cosine | `EMBEDDING_DIMENSIONS`, the `array<float, 384>` field type, and `schema::tests::the_vector_index_is_pinned_to_the_measured_dimension_and_distance` asserting against the live `INFO FOR TABLE` |
| ADV-STORE-004 §4.2 RocksDB optional | `rocksdb` cargo feature, off by default (measured: it adds ~10 min to a cold build here) |

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change)
- [x] test: scope-isolation, append-only-episodic, and provenance-required tests (red first)
- [x] test: TD-004 regression (string-bound cutoff must be impossible to express) and TD-002 EXPLAIN assertion
- [x] feat: schema (HNSW DIMENSION 384 DIST COSINE) + migrations + repositories to pass tests

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: scope-isolation bug leaks private context — highest-severity failure in
  the brief. Schema decisions here are the hardest to migrate later.
- Rollback: revert branch; drop dev database. No production data in v1 at this
  point.
- Risk (new): the plan assertions are load-bearing but fragile by nature — a
  SurrealDB upgrade or an added index can change the chosen operator and turn a
  real regression into a red test, or worse, hide one. The crate pins
  `surrealdb = "=3.2.4"` (workspace-wide, as of this advance) and deliberately
  defines **no index on `scope`**; see the note in `schema.rs`.

## Reviewability

`arrive score` reports **101 [RED]** (size 66, novelty 20, risk 15). Documented
rather than split, for one reason: every plausible split ships an unenforced
invariant.

- Schema without repositories leaves scope isolation as a comment. The schema
  cannot express it — SurrealDB `PERMISSIONS` do not apply to the privileged
  system user the store must be (TD-011), so the predicate has nowhere to live
  but the repository.
- Repositories without the schema's `EVENT`s put append-only on trust: the
  repository's refusal is one code path, and the whole point of the database
  rule is that it also binds the paths nobody has written yet.
- Tests split from either half stop being able to fail.

The three commits (`tidy` → `test` → `feat`) are the intended review order, and
the `test` commit is reviewable on its own as the specification. Two-thirds of
the size score is mechanical: `src/row.rs` (420 lines) is a field-by-field
mapping table, and the four test files are ~700 lines of fixtures and
assertions.

What was *not* pulled in to keep it this size: the access log (a `gateway`
invariant, ADV-GATEWAY-001), landscape ingestion (ADV-ARRIVE-SYNC-001), the
embedding pipeline (ADV-STORE-002), and promotion (ADV-CORE-003). The landscape
**tables and edges** are defined here because the advance asks for the schema;
nothing writes them yet.

## Evidence

- [x] tidy:preparatory — `tidy:` commit hoists the SurrealDB pin to the
      workspace manifest; the pre-existing suite passes unchanged
- [x] tdd:red-green — tests committed first and run red (25 failing, all
      `not implemented: ADV-STORE-001`), then green
- [x] tests:unit — 5 in-crate enforcement tests (`src/schema.rs`)
- [x] tests:integration — 21 tests across `tests/scope_isolation.rs`,
      `tests/lifecycle.rs`, `tests/schema_contracts.rs`
- [x] mutation checks — three deliberate breaks were run to prove the tests are
      not vacuous (see below)
- [ ] ci:passed — **not claimed.** CI runs on `advance/**` pushes; this record
      was written before the pipeline result for this branch was available.

Workspace verification at the time of writing: `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace` (**83 passed, 0 failed**) all clean;
`arrive doctor artifacts` 0 issues; `arrive plan check` valid.
`cargo build -p totem-store --features rocksdb` also builds (10m10s cold, which
is why the feature is off by default).

### Mutation checks

A test that passes against a deliberately broken implementation is worse than
no test, so each invariant was broken on purpose and the suite re-run:

| Break | Caught by |
|---|---|
| Scope predicate removed from the recall statement (Rust merge filter left in place) | `the_scope_predicate_is_generated_by_the_store_and_reaches_the_index_scan` — and *only* that test, which is precisely why the plan assertion exists |
| Scope predicate removed from both the statement and `get`, plus the defensive filter in `merge_chain` | 5 tests, across isolation, lifecycle, and the plan |
| Both append-only `EVENT`s removed from the schema | `the_database_refuses_to_update_an_episodic_row`, `the_database_refuses_to_delete_an_episodic_row` — the integration-level `episodic_records_refuse_revision` still passed on the Rust rule alone, which is the reason the in-crate tests exist |

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-STORE-001 --status passed`

## Partially established invariants

Stated plainly rather than implied by a green suite:

1. **"Every read and write appends to the access log" is not satisfied here.**
   That invariant belongs to `gateway` (see `components/gateway.yaml`) and
   ADV-GATEWAY-001 builds it. The store currently logs nothing, so a read
   through `totem-store` today leaves no audit trace. Until ADV-GATEWAY-001
   lands, the access-log invariant holds for no code path at all.
2. **Episodic metering is now impossible by construction.** The append-only
   `EVENT` refuses *every* update to an episodic row, including a
   `use_count` increment. ADV-CORE-002 and ADV-GATEWAY-004 must meter episodic
   retrieval in the access log rather than on the record. This is a real
   consequence of taking "never edited" literally, and it is the right side to
   err on for an audit substrate — but it is a constraint those advances
   inherit, not a detail.
3. **Scope isolation is enforced in two places within the store**, not one: the
   SurrealQL predicate, and a defensive precedence filter in `merge_chain`. The
   second exists because the cost of the first being wrong is another actor's
   private memory. The consequence is that a regression in the predicate alone
   would not change any *result*, which is exactly why the EXPLAIN assertion is
   a required part of this advance rather than a nicety.
4. **HNSW recall quality under a selective scope filter is unmeasured.** TD-003
   says five rows cannot establish an `ef` floor. `DEFAULT_SEARCH_EFFORT` is 40
   by inheritance from the spike, not by measurement; ADV-STORE-005 owns that.
5. **Server-mode behaviour is inherited, not re-executed.** ADV-STORE-006 closed
   parity on a workstation; this sandbox cannot run a `surreal` server, so every
   test here ran on the embedded `kv-mem` engine only.
6. **Dedup is a defined rule, not a semantic one.** Two records merge when
   category, subject, and whitespace/case-normalised body match. A paraphrase of
   the same instruction at two scopes will still surface twice; semantic
   consolidation is a curator job (ADV-CURATOR-001).

## Corrections to this advance

The advance as authored said the scope-chain merge resolves "with dedup and
precedence" without saying what makes two records duplicates. That is a decision,
not a detail — it is implemented as `(category, subject, normalised body)` and
recorded above so a reviewer can disagree with the rule rather than discover it.

No factual error was found in the advance's other claims; the TD/EMB references
all held.

## Changes Made

### 2026-08-05 - tidy: hoist the SurrealDB pin to workspace dependencies
- `Cargo.toml`: added `surrealdb = { version = "=3.2.4", default-features = false }`,
  `tokio`, and `futures` to `[workspace.dependencies]`, so `totem-store` cannot
  drift from the version ADV-STORE-004/006 measured their findings on
- `crates/totem-store-spike/Cargo.toml`, `crates/totem-mcp-spike/Cargo.toml`:
  point at the workspace deps and select only their own features. Resolution is
  identical and `Cargo.lock` is untouched

### 2026-08-05 - test: the store's invariants, before the store existed
- `crates/totem-store/tests/common/mod.rs`: fixtures — per-test embedded
  `kv-mem` instance, actor/team chains, 384-dimension unit vectors
- `crates/totem-store/tests/scope_isolation.rs`: 7 tests — cross-actor reads,
  absent-not-forbidden, refused out-of-chain writes, team membership, chain
  merge precedence, and the EXPLAIN assertion that the predicate reaches the
  index scan
- `crates/totem-store/tests/lifecycle.rs`: 7 tests — episodic revision refused,
  revisable categories rewritten, foreign records unrevisable, scope unchanged
  by revision, provenance/economics/governance round trip, all six categories,
  subject links (including a slash-bearing repo id)
- `crates/totem-store/tests/schema_contracts.rs`: 7 tests — migration
  idempotence and ordering, TD-004 temporal filtering, TD-002 index usage,
  unembedded records still recallable, dimension refusals, category/limit
- `crates/totem-store/src/schema.rs`: 5 in-crate tests issuing raw statements —
  episodic UPDATE/DELETE refused, provenance-free row refused, a revisable-row
  control, and the live index definition matched against `EMBEDDING_DIMENSIONS`

### 2026-08-05 - feat: schema, migrations, and scope-isolating repositories
- `crates/totem-store/src/schema.rs`: `MEMORY_SCHEMA_V1` — SCHEMAFULL `memory`
  with required provenance, `array<float, 384>` embeddings, the HNSW index, the
  two append-only `EVENT`s, and the landscape tables/edges (`repo`, `system`,
  `component`, `advance`, `actor`, `impacts`, `depends_on`, `owned_by`)
- `crates/totem-store/src/migrate.rs`: `Migration`/`MIGRATIONS`/`AppliedMigration`
  — forward-only, append-only list
- `crates/totem-store/src/store.rs`: `Store`, the embedded constructor, the
  ledger-driven migration runner, and the `#[cfg(test)]`-only connection accessor
- `crates/totem-store/src/memory.rs`: `RecallQuery` (typed cutoff, knn probe,
  categories, limit), `MemoryRepository` (`save`/`get`/`revise`/`recall`/
  `explain_recall`), the scope-predicate generator, and `merge_chain`
- `crates/totem-store/src/row.rs`: the explicit domain↔row mapping, including
  the single `DateTime<Utc>` → `Datetime` conversion point
- `crates/totem-store/src/error.rs`: `StoreError`, deliberately without a
  "found but forbidden" variant
- `crates/totem-store/Cargo.toml`, `Cargo.toml`: the crate, its optional
  `rocksdb` feature, and its workspace membership

## Check for Understanding

1. `MemoryRepository::get` returns `Ok(None)` for a record in another actor's
   scope, while `save` returns `StoreError::ScopeDenied` for a write into one.
   Explain why the read and the write are deliberately asymmetric, and what a
   caller could learn if `get` returned `ScopeDenied` instead.
2. `merge_chain` in `src/memory.rs` re-checks `reader.precedence_of(&record.scope)`
   even though the SurrealQL statement already filtered on the same chain. The
   mutation table above shows that removing the SQL predicate alone changes no
   result. Given that, argue either that the second check should stay or that it
   should go — and say which test is doing the real work in each case.
3. `schema.rs` enforces append-only with two `DEFINE EVENT`s, and
   `MemoryRepository::revise` *also* refuses episodic categories via
   `MemoryRecord::revise`. What does each layer catch that the other does not?
   Name a concrete future code path that only the database rule would stop.
4. The append-only event refuses every `UPDATE` to an episodic row, including a
   `use_count` increment. Solution Intent §4 says every memory carries retrieval
   counters. Which of the two gives way, where does the episodic count have to
   live instead, and which advance inherits that constraint?
5. `RecallQuery::statement` formats `K` and `ef` directly into the SQL while
   binding scopes, categories, the cutoff, and the probe as parameters. Justify
   the split. What property of `K` and `ef` makes the formatting safe, and what
   would have to change for it to stop being safe?
6. `schema.rs` deliberately defines no index on `scope`, and says so in a
   comment. What would adding `DEFINE INDEX memory_scope ON memory FIELDS scope`
   plausibly do to `tests/schema_contracts.rs::vector_recall_uses_the_hnsw_index_rather_than_a_full_scan`,
   and why is that a reason to defer the index rather than a reason never to add
   it?
7. `row.rs` maps every domain enum to a string by hand instead of reusing the
   `serde` representations `totem-core` already derives. Name the two failure
   modes this avoids — one about types, one about renames — and say which one
   TD-004 is about.
8. `Store::connection()` is `#[cfg(test)]`. `tests/scope_isolation.rs` therefore
   cannot reach it, but `src/schema.rs`'s tests can. Explain what that split buys
   and what it costs, and say where a future test proving that a *curator* job
   cannot bypass scope would have to live.
9. `dedup_key` treats "Run `cargo fmt` before pushing" at project scope and
   "run `cargo fmt`   BEFORE pushing" at actor scope as the same fact. Give a
   pair of records where this rule merges two things that should have stayed
   separate, and say which category it would most likely bite in.
10. The `## Reviewability` section argues this Red-scoring (101) advance should
    not be split. If you disagree, which of `src/row.rs`, `src/memory.rs`, or the
    schema's `EVENT`s would you land in a separate sub-PR first — and what
    invariant would be unenforced on the phase branch in the interval?
