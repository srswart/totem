---
advance:
  id: "ADV-GATEWAY-008"
  title: "Enablement: evaluation harness (workload driver + recall-quality scorer)"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T19:30:11Z"
  review_time_estimate_minutes: 35
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 43
  risk_flags: ["public_api"]
  evidence:
    ["profile:selected-practices", "enablement:artifact", "tests:integration"]
  practices:
    tidy_first:
      status: not_applicable
      rationale: "No production_code work product declared; the one tidy: commit in this advance's series is a cargo-fmt reflow, not a preparatory refactor of prior behavior — there was no prior behavior in this module to refactor."
    tdd:
      status: not_applicable
      rationale: "work_products is [performance_harness, test_automation], not production_code (arrive-advance-profiles.md), so tdd:red-green is not claimed even though tests/eval_workload.rs and tests/eval_quality.rs were committed before crates/totem-gateway/src/eval/ existed and failed with E0433 (could not find `eval` in `totem_gateway`). automation_control_validation and performance_measurement_integrity (below) are what the profile asks for instead."
    automation_control_validation:
      status: applied
      rationale: "Both eval_quality.rs tests and both eval_workload.rs tests are positive/negative control pairs, not single assertions: the golden reader scores precision_at_1=1.0/recall_at_k=1.0 while a reader outside the corpus's project scope scores 0.0/0.0 (quality); a clean baseline profile reports zero errors while an injected-latency profile visibly raises every latency figure and lowers throughput (workload). Oracle: totem_store::corpus's already-reviewed golden query set (ADV-STORE-005) for quality; a deterministic injected Duration for workload, so neither control depends on the host's own performance."
    performance_measurement_integrity:
      status: applied
      rationale: "Every WorkloadReport carries an EnvironmentStamp (OS, arch, available_parallelism, captured_at) alongside the latency/throughput figures, so a number is never reported without saying what produced it. LatencyStats reports min/mean/p95/max rather than one number that could hide a bimodal distribution."
  model_usage: []
  schema_version: 2
  mode: enablement
  facets: [software, quality, performance]
  work_products: [performance_harness, test_automation]
  status: complete
---

## Objective

Build the evaluation tooling the quality and performance evaluations run:
(1) a workload driver that replays agent-turn traffic (recall/save mixes over
MCP and REST) against a seeded instance with configurable concurrency, and
(2) a recall-quality scorer that runs the golden query set from ADV-STORE-005
and reports ranking metrics (e.g. precision@k, expected-item rank).

## Outcome

After this advance:
- One command runs a defined workload profile and emits latency/throughput
  measurements with environment metadata captured alongside (required for
  performance evidence to be determinate, per the performance protocol).
- One command scores recall quality against the golden set and emits a
  comparable report, so ranking changes (ADV-CORE-002) are measurable, not
  vibes.
- Both are proven sensitive: a deliberately degraded configuration (negative
  control) visibly worsens the reported numbers, and a known-good baseline
  (positive control) reports as expected.

## Planned Work

- [x] branch: create or confirm feature branch for this advance
- [x] feat: workload driver (profiles, concurrency, latency capture, environment stamp)
      — `totem_gateway::eval::workload::run_workload`
- [x] feat: recall-quality scorer over the golden query set —
      `totem_gateway::eval::quality::score_recall_quality`
- [x] prove sensitivity: negative + positive control runs documented —
      `crates/totem-gateway/tests/eval_workload.rs`,
      `crates/totem-gateway/tests/eval_quality.rs`
- [x] operationalize: runnable locally and from CI — `cargo test -p
      totem-gateway --test eval_workload --test eval_quality` (a library
      call, not a standalone CLI binary; see Corrections below)

## Bug Fixes

- [ ] None yet

## Corrections to the Advance

- **"One command" is a library call, not a terminal command**, for both the
  workload driver and the quality scorer — the same scoping choice
  ADV-STORE-005 made for its corpus generator, and for the same reason: the
  declared immediate consumers (ADV-CORE-005, ADV-GATEWAY-006,
  ADV-GATEWAY-007) are all Rust code that will call
  `totem_gateway::eval::{workload, quality}` directly. What was built is
  `run_workload(&state, &profile)` and `score_recall_quality(&store,
  reader_override)`, each proven end-to-end by its own test file. A thin CLI
  wrapper printing JSON is a small addition any of those advances — or a
  future workstation advance — can add on top of this module without
  touching the harness itself. Recorded here rather than silently narrowed.
- **The workload driver measures the shared `ops` layer, not either
  transport separately.** The Objective says "recall/save mixes over MCP and
  REST"; `run_workload` calls `ops::recall` / `ops::save` directly — the
  same functions `ops.rs`'s own doc comment says both `handlers.rs` (REST)
  and `mcp.rs` (MCP) call with no scope-relevant logic of their own. One
  measurement here is therefore representative of both surfaces' shared
  cost. What it does not capture is HTTP/MCP marshalling overhead itself —
  out of scope for this harness, a separate concern from the store/access-log
  cost this module isolates.

## Risk + Rollback

- Risk: a harness that measures the wrong thing quietly blesses regressions
  — mitigated by the mandatory control runs (`tests/eval_workload.rs`,
  `tests/eval_quality.rs`): each proves the harness moves in the expected
  direction under a deliberately degraded input, not just that it runs
  without error.
- Risk (`public_api`): `pub mod eval` is a new public surface on
  `totem-gateway`, reachable by any future consumer of this crate, not only
  the evaluation advances that currently call it.
- Rollback: harness is tooling only; revert without production impact — no
  migration, no schema change, no effect on `/recall` or `/save` themselves.

## Evidence

- [x] profile:selected-practices — `automation_control_validation` and
      `performance_measurement_integrity` applied; `tidy_first`/`tdd`
      recorded `not_applicable` with rationale in the frontmatter
      `practices` block.
- [x] enablement:artifact — `crates/totem-gateway/src/eval/workload.rs` (the
      driver), `crates/totem-gateway/src/eval/quality.rs` (the scorer), and
      `crates/totem-gateway/src/eval/mod.rs` (module docs), plus
      `crates/totem-gateway/tests/eval_workload.rs` and
      `crates/totem-gateway/tests/eval_quality.rs` (the sensitivity proofs).
- [x] tests:integration — `cargo test --workspace`: 348 tests pass (incl.
      doctests), of which 4 are this advance's own; zero regressions in the
      other 344.
- Not claimed: `tdd:red-green` — see `practices.tdd` above.
- Not claimed: `ci:passed` — no pipeline result has been observed for this
  branch yet.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-GATEWAY-008 --status passed`
- Checks run locally on the branch: `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `arrive doctor artifacts`, `arrive plan check`, `arrive check --strict`,
  `arrive score` — all green.

## Reviewability

`arrive score` reports **43 [YELLOW]** (size 33, novelty 10, risk 0)
measured against the sub-branch's own base (the claim commit). Kept whole
rather than split further: the natural tidy/test/feat boundary is already
the commit structure (a `test:` commit that fails to compile alone, a
`feat:` commit that makes it pass, a `tidy:` cargo-fmt commit), and splitting
into two *sub-PRs* — the unit this budget actually measures — is not
available to a single advance under this repo's phase/sub-PR discipline. It
would not help here regardless: the workload driver and the quality scorer
are two halves of one Outcome ("both are proven sensitive"), and a
scorer-only or driver-only sub-PR would leave the other consumer advance
(ADV-CORE-005 needs quality; ADV-GATEWAY-007 needs workload) still blocked.

## Changes Made

### 2026-08-06 - test: sensitivity proofs for workload driver + quality scorer
- crates/totem-gateway/tests/eval_workload.rs: new — positive control
  (baseline profile reports clean, error-free numbers with both recall and
  save samples) and negative control (an injected per-operation delay
  visibly raises recall/save mean latency and lowers throughput)
- crates/totem-gateway/tests/eval_quality.rs: new — positive control (the
  golden query set's own reader scores precision_at_1=1.0, recall_at_k=1.0)
  and negative control (a reader outside the corpus's project scope scores
  0.0/0.0)
- Committed before `crates/totem-gateway/src/eval/` existed; failed with
  `E0433: could not find eval in totem_gateway`

### 2026-08-06 - feat: evaluation harness (workload driver + recall-quality scorer)
- crates/totem-gateway/src/eval/workload.rs: new — `WorkloadProfile`
  (recall/save mix via `mix_period`, `concurrency`, `injected_latency`),
  `EnvironmentStamp` (OS/arch/available_parallelism/captured_at),
  `LatencyStats` (count/min/mean/p95/max), `WorkloadReport`, and
  `run_workload()`, which drives `ops::recall` / `ops::save` directly with
  bounded concurrency via `tokio::task::JoinSet`, reading/writing as
  `corpus::NOVA` against `corpus::ROCKET` project scope
- crates/totem-gateway/src/eval/quality.rs: new — `QueryResult`,
  `QualityReport`, and `score_recall_quality()`, which runs
  `totem_store::corpus::golden_queries()` against a seeded store with an
  overridable reader `ScopeChain`, reporting `precision_at_1` and
  `recall_at_k`
- crates/totem-gateway/src/eval/mod.rs: new — module docs, the
  "operationalize" scoping note (Corrections above)
- crates/totem-gateway/src/lib.rs: `pub mod eval;`

### 2026-08-06 - tidy: cargo fmt
- crates/totem-gateway/src/eval/{workload,quality}.rs,
  crates/totem-gateway/src/lib.rs, crates/totem-gateway/tests/eval_workload.rs:
  reflow only, no behavior change

### 2026-08-06 - docs: complete ADV-GATEWAY-008
- arrive/systems/058-totem-core/advances/ADV-GATEWAY-008.md: status
  complete, evidence, practice dispositions, two Corrections entries
  (library calls instead of CLI binaries; shared-`ops`-layer measurement
  instead of separate REST/MCP measurement), Reviewability justification,
  refreshed CFU
- arrive/implementation-plan.yaml: plan item ADV-GATEWAY-008 set to done

## Check for Understanding

1. `run_workload` builds one `Caller::Trusted` and clones it into every
   spawned task, and reads/writes as `corpus::NOVA` against
   `corpus::ROCKET` regardless of `WorkloadProfile`. What would have to
   change for this driver to measure a workload with more than one actor
   or project in its mix, and why doesn't the current shape need that?
2. `injected_latency_visibly_worsens_every_latency_figure` asserts
   `degraded.recall.mean_ms > baseline.recall.mean_ms` rather than a fixed
   threshold like `> 25.0`. Why is a relative comparison the right
   assertion for a performance sensitivity proof running in a shared CI
   environment, and what would a fixed-threshold assertion risk instead?
3. `score_recall_quality`'s `reader_override` replaces every golden query's
   reader chain with one fixed chain when set. The negative-control test
   passes `ScopeChain::resolve(&ActorId::new(JUNIPER)..., None, &[])` — no
   project, no teams. Walk through why that specific choice (rather than,
   say, overriding only the actor and keeping `query.reader_project`) is
   what drives `precision_at_1` and `recall_at_k` to exactly `0.0` instead
   of some smaller-but-nonzero number.
4. `WorkloadReport.throughput_ops_per_sec` divides `total_ops` by
   `total_duration`, where `total_ops` includes `error_count`. If a future
   caller ran a profile against an unmigrated store (every operation
   erroring immediately), what would this report's throughput number look
   like, and would it correctly signal "this run is not usable" on its
   own — or does a reader need to check `error_count` too?
5. The frontmatter records `tdd` as `not_applicable` even though
   `tests/eval_workload.rs` and `tests/eval_quality.rs` were committed in a
   commit before `crates/totem-gateway/src/eval/` existed and failed with
   `E0433`. Reconcile those two facts using
   `arrive-advance-profiles.md`'s work-product rule — and contrast with
   why `ADV-STORE-001` (a `production_code` advance) would be required to
   make the opposite claim under the same commit shape.
6. `score_one` compares `record.content.body == expected` by exact string
   equality against a `&'static str` golden fixture body. `NEAR_DUP_A` and
   `NEAR_DUP_B` are two near-duplicate but non-identical bodies in the
   corpus (ADV-STORE-005). Why does exact-string `must_appear` matching
   still work correctly for that query, given the two bodies were never
   deduplicated into one record?
7. The Objective's workload driver description says "over MCP and REST",
   but `run_workload` calls `ops::recall` / `ops::save` directly. Given
   who this harness's declared consumers are (ADV-GATEWAY-007, a
   performance evaluation), is the Corrections section's justification —
   that both surfaces call these same functions with no scope-relevant
   work of their own — sufficient, or does a performance evaluation
   specifically need transport-level numbers this harness cannot produce?
