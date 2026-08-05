---
advance:
  id: "ADV-STORE-004"
  title: "Investigation: SurrealDB multi-model one-round-trip spike"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-05T07:50:58Z"
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: ["https://github.com/srswart/totem/pull/2"]
  external_refs: []
  reviewability_score: 83
  risk_flags: ["concurrency"]
  evidence: ["profile:selected-practices", "investigation:findings", "tests:integration"]
  practices:
    tidy_first:
      status: not_applicable
      rationale: "Investigation mode, no production_code work product — the spike crate is evidence that ADV-STORE-001 supersedes, and there was no existing code to prepare."
    tdd:
      status: not_applicable
      rationale: "Investigation mode, no production_code work product. The assertions were derived from observed engine behaviour, not written ahead of an implementation; no tdd:red-green is claimed. Sensitivity was established by negative controls instead (see Evidence)."
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software]
  work_products: []
  status: complete
---

## Objective

Validate the load-bearing assumption behind G2 and the read→think→write turn
model (Solution Intent §1): that one SurrealQL round trip can assemble graph
traversal + vector search + temporal facts into complete context, and that
decisions, entity updates, and triggered events commit in one ACID
transaction. Also exercise live queries (console feed) and the embedded vs.
server deployment modes.

Findings are recorded in [docs/tech-direction/surrealdb.md](../../../../docs/tech-direction/surrealdb.md).

## Outcome

The assumption is **confirmed**, not refuted, so no two-query fallback is needed
and its cost is not incurred. A single `SELECT` returns vector-ranked rows each
carrying a graph traversal, a resolved record link, a temporal cutoff and a scope
filter; a turn's three writes commit or abort as one transaction; live queries
fire on the embedded engine and stay silent for rolled-back writes.

The spike also turned up four constraints that later advances must honour to keep
the assumption true: the two knn syntaxes are not interchangeable — only
`<|K,EF|>` uses the vector index (TD-002); a temporal cutoff bound as a string
silently filters nothing (TD-004); `vector::distance::cosine` is not callable in
3.2.4 (TD-005); and live notifications are **not ordered by statement within a
transaction** (TD-008). Alongside them sit three confirmations — the round trip
itself (TD-001), predicates being pushed into the index scan (TD-003), and
transaction atomicity (TD-006/TD-007) — all written up with the observed query
plans and error text.

Engine parity is **partially closed** — see Risk + Rollback.

## Planned Work

- [x] branch: create or confirm feature branch for this advance
- [x] spike: toy schema with document + graph + vector + temporal data
- [x] spike: one-round-trip recall query; multi-write ACID transaction; live query
- [~] verify engine parity: every capability above (esp. live queries and vector
      indexes) behaves the same on the embedded in-memory engine (`kv-mem`,
      what all tests use) as on server mode (what production will use);
      document any capability that only works against a server
      — **partial**: no `surreal` server can be run in this sandbox (the egress
      proxy refuses `download.surrealdb.com` and `install.surrealdb.com` with
      `CONNECT tunnel failed, response 403`). A runnable parity test exists
      behind the `server-parity` feature; it compiles but has never executed
      against a server. The HTTP-protocol live-query limitation was established
      from the SDK source instead.
- [x] write findings incl. version/feature caveats to docs/tech-direction/surrealdb.md

## Bug Fixes

- [ ] None — no defect in existing repo code. The string-vs-`Datetime` binding
      trap (TD-004) is a SurrealQL semantic caught before any store code exists,
      not a fix to something already written.

## Risk + Rollback

- Risk (realised, contained): the spike could have refuted the assumption late.
  It ran before ADV-STORE-001 commits to a schema, which is exactly why. It
  confirmed the assumption.
- Risk (open): **server-mode parity is expected but unverified.** Query
  semantics are argued to be identical because `engine::local` and `surreal
  start` both execute through `surrealdb_core::kvs::Datastore`; auth and
  capability defaults genuinely differ and are unchecked. Carried forward to
  ADV-STORE-001 and ADV-GATEWAY-003.
- Risk (open): HNSW recall under a *selective* scope filter on a realistic
  corpus is unmeasured — the toy table has five rows. Deferred to ADV-STORE-005.
- Risk flag `concurrency`: the live-query experiment reads an async notification
  stream, and took two corrections to become sound. It first drained until quiet
  — passing alone, failing under concurrent test load — and now waits for a
  committed sentinel record, so the assertion no longer depends on a timeout. It
  then flaked roughly one run in four on notification *order*, which turned out
  to be the engine's behaviour rather than the test's: intra-transaction
  notifications are unordered (TD-008). The assertion is now order-insensitive
  within a transaction, and 15 consecutive runs are clean. Widening a timeout
  would have hidden a finding the console depends on.
- Rollback: findings only. `crates/totem-store-spike` is an isolated workspace
  member with no dependents; deleting it and its workspace entry removes the
  whole change. `totem-store` derives its own schema rather than importing this
  one.

## Evidence

- [x] profile:selected-practices — investigation mode, empty `work_products`;
      `tidy_first` and `tdd` recorded `not_applicable` with rationale above. No
      `tdd:red-green` claimed.
- [x] investigation:findings — docs/tech-direction/surrealdb.md, TD-001…TD-008,
      each tied to an executed experiment or an explicitly-labelled source read.
- [x] tests:integration — `cargo test --workspace`: 50 tests pass, of which 6 are
      the spike's experiments against the embedded `kv-mem` engine.
- Negative controls (sensitivity, in place of TDD's red phase):
  - removing `scope IN $scopes` from the recall statement fails 4 of the 6
    experiments, including the isolation assertion;
  - making the deliberately-failing transaction valid fails both the atomicity
    and the live-feed assertions (re-checked after the assertion was loosened
    for TD-008, so the looser form still bites).
- Stability: 15 consecutive runs of the spike suite pass after the TD-008 fix.
- Not claimed: `ci:passed` — no pipeline result was observed for this branch.
- Not claimed: any performance number. Timings in the findings come from a
  five-row table and are not a workload model.

## CI Evidence Notes

- Findings document is the primary artifact; no pipeline evidence expected.
- Checks run locally on the branch: `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `arrive doctor artifacts`, `arrive plan check`, `arrive check --strict`,
  `arrive score`.

## Reviewability

`arrive score` reports **83 [RED]** (size 58, novelty 20, risk 5) measured
against `origin/master`. Re-running it later on the branch scores only the delta
since the last push, which is not the figure below. The change is
kept whole rather than split, because 4,708 of its ~5,545 changed lines are the
regenerated `Cargo.lock` for a single pinned dependency — the entire SurrealDB
tree arriving at once. The hand-written surface is ~1,080 lines, over half of it
prose:

| File | Lines | Splittable? |
|---|---|---|
| `Cargo.lock` | 4,708 | No — generated, one dependency |
| `crates/totem-store-spike/src/lib.rs` | 366 | No — the five experiments share one schema and seed |
| `docs/tech-direction/surrealdb.md` | 240 | No — the findings are the deliverable |
| `arrive/.../ADV-STORE-004.md` | 204 | No — this record |
| `crates/totem-store-spike/tests/*.rs` | 228 | No — assertions over that shared fixture |
| `Cargo.toml` × 2, `examples/plan.rs`, plan | 42 | No |

Splitting by experiment would land a schema with no assertions, then assertions
with no findings — each reviewable in isolation but none of them able to answer
the question the advance exists to answer. Reviewers can read the findings
document first and treat the crate as its appendix; the lockfile needs no line
review beyond confirming the pin is `=3.2.4`.

## Changes Made

### 2026-08-05 - feat: add the ADV-STORE-004 SurrealDB investigation spike
- Cargo.toml: added `crates/totem-store-spike` as a workspace member
- Cargo.lock: regenerated for `surrealdb =3.2.4` (exact pin), `futures`, `tokio`
- crates/totem-store-spike/Cargo.toml: pinned `surrealdb =3.2.4` with only
  `kv-mem`; added the off-by-default `server-parity` feature that enables
  `surrealdb/protocol-ws`
- crates/totem-store-spike/src/lib.rs: toy schema (document + HNSW vector index
  + graph relation + record link + datetime), seed data arranged so foreign-scope
  records are *closer* to the probe than readable ones, and five experiments
  written generic over `Connection` so both engines run the same assertions
- crates/totem-store-spike/tests/embedded.rs: the six assertions against the
  embedded `kv-mem` engine; each test builds its own instance and seeds itself
- crates/totem-store-spike/tests/server_parity.rs: the same experiments over
  WebSocket, behind the off-by-default `server-parity` feature so a default
  workspace test run can never wait on a server
- crates/totem-store-spike/examples/plan.rs: prints the `EXPLAIN FULL` plan
  quoted in the findings, so a reviewer can regenerate it

### 2026-08-05 - docs: record the SurrealDB spike findings as tech direction
- docs/tech-direction/surrealdb.md: new — verdict, TD-001…TD-007 with observed
  query plans and error text, the 5m02s cold-build cost, seven constraints
  handed to ADV-STORE-001, and the parity section stating plainly what was
  executed versus read from source

### 2026-08-05 - fix: record TD-008 and stop asserting live-feed order
- crates/totem-store-spike/tests/embedded.rs: the live-feed assertion flaked one
  run in four on notification order; loosened to be order-insensitive within a
  transaction while still requiring both writes, the sentinel last, and no
  rolled-back record
- crates/totem-store-spike/tests/server_parity.rs: same loosening, so parity is
  compared against the behaviour that actually holds
- docs/tech-direction/surrealdb.md: added TD-008 — intra-transaction live
  notifications are unordered; consumers must order by a field the records carry

### 2026-08-05 - docs: complete ADV-STORE-004
- arrive/systems/058-totem-core/advances/ADV-STORE-004.md: status complete,
  evidence, practice dispositions, Reviewability justification, refreshed CFU
- arrive/implementation-plan.yaml: plan item ADV-STORE-004 set to done

### 2026-08-05 - fix: address PR review on the spike's soundness

Three points raised by an automated reviewer on PR #2, all accepted:

- crates/totem-store-spike/src/lib.rs: `verify_live_query` defaulted a missing
  notification `id` to an empty string, which would have made the "no
  rolled-back write reached the feed" assertion pass vacuously. It now panics
  with the offending payload — the whole point of that assertion is proving an
  absence, so a silent fallback defeated it.
- crates/totem-store-spike/tests/server_parity.rs: with `server-parity` enabled
  but no `TOTEM_SPIKE_SURREAL_URL`, the test returned `Ok(())` after printing a
  skip notice that `cargo test` captures — so parity could report a pass having
  checked nothing. Enabling the feature is the opt-in; a missing URL now fails
  fast with the command needed to fix it.
- docs/tech-direction/surrealdb.md: TD-002's heading said `<|K,DIST|>` while its
  body measured `<|K,COSINE|>`. Restated so the rule (a number is HNSW `ef`, a
  distance name is brute force) is unambiguous, and corrected the verdict line,
  which listed TD-006 among the constraints and omitted TD-008.

## Check for Understanding

1. `RECALL_QUERY` uses `embedding <|3,40|> $probe`, not `<|3,COSINE|>`. What
   changes in the query plan if you swap them, why do both return correct rows
   on this dataset, and what does that imply about the tests ADV-STORE-001 needs
   around its own recall query?
2. The seed in `src/lib.rs` gives the two `foreign_*` records embeddings *closer*
   to the probe than any readable record. What would `recall_never_returns_another_actors_scope`
   fail to detect if they were further away instead?
3. `scope_predicate_is_pushed_into_the_index_scan` asserts on `EXPLAIN FULL`
   output rather than on rows. What regression does that catch that a row-level
   assertion cannot — and what does it still *not* prove about HNSW recall at
   corpus scale?
4. TD-004 says a temporal cutoff bound as a `String` filters nothing and raises
   no error. Where in a future `totem-store` query builder would that bug most
   plausibly appear, and what makes it dangerous beyond returning extra rows?
5. `verify_live_query` commits a sentinel record and reads until it arrives,
   rather than draining the stream until quiet. What failure did the drain-until-
   quiet version have, and why is the sentinel version not merely a longer
   timeout?
6. The live-feed assertion in `tests/embedded.rs` checks that both writes are
   present but not the order they arrive in, while requiring the sentinel to be
   last. Why is that not a weakened test — and what would a console reducer
   driven by feed order get wrong (TD-008)?
7. `verify_live_query` panics when a notification carries no `id`, and
   `server_parity` panics when its URL is unset — in both cases where an earlier
   version returned quietly. What do those two failures have in common, and what
   would each have concealed?
8. The advance claims `investigation:findings` and `tests:integration` but not
   `tdd:red-green`. Point to the frontmatter fields that make that the correct
   claim, and say what stands in for TDD's red phase here.
9. `tests/server_parity.rs` compiles but has never run. What exactly is still
   unverified about server mode, what argument does the findings document give
   for expecting parity anyway, and which later advance is most exposed if that
   argument is wrong?
