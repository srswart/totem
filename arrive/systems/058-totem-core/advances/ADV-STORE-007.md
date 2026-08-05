---
advance:
  id: "ADV-STORE-007"
  title: "Investigation: recommended embedding model executed and measured (closes ADV-STORE-003's residual)"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store"]
  started_at: "2026-08-05T11:20:00Z"
  implementation_completed_at: "2026-08-05T11:29:55Z"
  review_time_estimate_minutes: 20
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 108
  risk_flags: []
  evidence: ["profile:selected-practices", "investigation:findings", "tests:unit", "automation:control-validation"]
  practices:
    automation_control_validation:
      status: applied
      rationale: "Dimension asserted at run time (384); ranking asserted per query; sensitivity control in the same run proves the lexical baseline still fails the paraphrase query under the identical harness, so the semantic-vs-lexical comparison retains its discriminating case; two consecutive runs reproduce ranking and latency."
    tidy_first:
      status: not_applicable
      rationale: "Investigation mode; additions are feature-gated test automation in the throwaway spike crate, no production code touched."
    tdd:
      status: not_applicable
      rationale: "test_automation work product, not production_code; assertions encode expected model behaviour verified by execution, not a red phase ahead of an implementation. No tdd:red-green claimed."
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software]
  work_products: [test_automation]
  status: complete
---

## Objective

Close the residual risk ADV-STORE-003 carried forward: its recommendation
(local pretrained model, BGE-small-en-v1.5 via `fastembed`) rested on
published specs, because the cloud sandbox blocks the model-weight download
(EMB-002/EMB-003). Execute the recommended model on a workstation with hub
access — the same environment split ADV-STORE-006 used for SurrealDB server
parity — against the identical corpus, labeled queries, and harness the
lexical baseline was measured with, **before** ADV-STORE-001 pins the vector
index dimension and ADV-STORE-002 implements the pipeline.

## Outcome

Executed 2026-08-05 on an Apple-silicon workstation (EMB-004 in
[docs/tech-direction/embeddings.md](../../../../docs/tech-direction/embeddings.md)):

- **5/5 labeled queries rank their expected record first — including the
  paraphrased query the hashing baseline ranks second** (EMB-001's
  discriminating case). The recommendation is now measured, not argued.
- **Dimensionality 384 asserted at run time** — ADV-STORE-001's
  `DIMENSION 384` index pin now rests on an executed assertion.
- **Latency ~5 ms per embedding** (CPU ONNX, debug build) — comfortably
  inside a synchronous gateway-write budget. Model construction ~276 s on a
  cold cache (one-time download) vs ~124 ms warm: load once at startup,
  bake weights into sandboxed deployment images.
- The embeddings tech-direction doc is retitled closed; the residual risk on
  ADV-STORE-002 is resolved for the recommended candidate (API candidates
  stay unmeasured, accepted — the privacy ground disfavors them regardless).

This advance also carries the incorporation of all investigation findings
into the next advances' specs (see Changes Made): the DIMENSION 384 pin and
TD constraints into ADV-STORE-001, the Embedder-trait/sandbox-split guidance
into ADV-STORE-002, TD-009 connection constraints into ADV-GATEWAY-001 /
ADV-CONSOLE-001, the TD-011 connection-identity invariant onto the `store`
component, and `docs/tech-direction/` into the cloud agents' binding reading
list.

## Planned Work

- [x] branch: create or confirm feature branch for this advance
- [x] add the recommended model behind an off-by-default `local-model` feature
      (sandbox `cargo test --workspace` must never attempt a download)
- [x] run the ADV-STORE-003 harness against it; record quality, dimension,
      latency; prove sensitivity with the lexical baseline as in-run control
- [x] update docs/tech-direction/embeddings.md (EMB-004, verdict, pin)
- [x] incorporate findings into ADV-STORE-001/002, ADV-GATEWAY-001,
      ADV-CONSOLE-001, store invariants, and the cloud notes reading list

## Bug Fixes

- [ ] None — no defect in existing code; one workspace lint (missing Debug
      impl) fixed before commit.

## Risk + Rollback

- Risk (resolved): the model pin could have rested on spec sheets; it now
  rests on a measurement, though on a 10-record calibration corpus —
  corpus-scale quality remains ADV-STORE-005's question.
- Risk (open, minor): latency was measured on Apple-silicon debug build;
  deployment hardware will differ. The ~5 ms figure is a budget sanity check,
  not a performance evidence claim (that is ADV-GATEWAY-007's job).
- Rollback: findings + feature-gated test automation in a throwaway spike
  crate with no dependents; spec amendments are documentation.

## Evidence

- [x] profile:selected-practices — investigation mode, `test_automation` work
      product; `automation_control_validation` applied, `tdd`/`tidy_first`
      not_applicable with rationale in frontmatter. No `tdd:red-green` claimed.
- [x] investigation:findings — EMB-004 in docs/tech-direction/embeddings.md,
      tied to the executed run with verbatim numbers.
- [x] tests:unit — `cargo test -p totem-embedding-spike --features
      local-model`: 1/1 (plus the 3 existing offline tests unaffected);
      default `cargo test --workspace` builds none of it.
- [x] automation:control-validation — dimension assertion, per-query ranking
      assertions, in-run lexical-baseline sensitivity control, 2 consecutive
      reproducing runs.
- Not claimed: `ci:passed` (CI cannot download the model); any corpus-scale
      quality or deployment-hardware latency number.

## CI Evidence Notes

- The `local-model` test cannot run in CI or the cloud sandbox (EMB-002). Do
  not add it to any default test invocation. Checks run on the workstation:
  `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` (plus the spike with `--features local-model`), `cargo test
  --workspace`, `arrive doctor artifacts`, `arrive plan check`,
  `arrive check --strict`, `arrive score`.

## Reviewability

`arrive score` reports **108 [RED]** (size 73, novelty 15, risk 20) against
master. Kept whole, per the ADV-STORE-004 precedent: **1,500 of the ~1,619
inserted lines are the regenerated `Cargo.lock`** for a single pinned optional
dependency (`fastembed =5.17.4` pulls the ONNX/tokenizer tree). The
human-reviewable surface is ~120 lines: one provider module, one test, one
findings section, and spec amendments. Splitting would separate the
measurement from the spec changes it justifies.

Risk flags are heuristic hits on prose, noted rather than silently disputed:
`auth` fires on the TD-011/authorization wording in spec amendments (no auth
code exists yet); `caching` on the model weight-cache discussion; `concurrency`
on the `Mutex` wrapper — the only one touching real code, and it exists solely
to adapt fastembed's `&mut self` to the shared-ref provider trait in a
single-threaded measurement harness.

## Changes Made

### 2026-08-05 - test: run the recommended embedding model behind an opt-in feature
- crates/totem-embedding-spike/Cargo.toml: optional pinned `fastembed =5.17.4`
  behind new `local-model` feature
- crates/totem-embedding-spike/src/local_model.rs: `FastembedProvider`
  (BGE-small-en-v1.5, Mutex-wrapped for the shared-ref provider trait,
  load-time captured separately from per-call latency)
- crates/totem-embedding-spike/src/lib.rs: module gate + `pub(crate)`
  l2_normalize
- crates/totem-embedding-spike/tests/local_model_quality.rs: quality,
  dimension, latency assertions + lexical-baseline sensitivity control

### 2026-08-05 - docs: pin the embedding decision and incorporate all findings
- docs/tech-direction/embeddings.md: EMB-004, verdict closed, model/dimension
  pin, residual resolved
- arrive/.../advances/ADV-STORE-001.md: binding constraints section
  (TD refs, DIMENSION 384, TD-011 connection identity), two task additions
- arrive/.../advances/ADV-STORE-002.md: measured pin + Embedder-trait
  sandbox-split guidance
- arrive/.../advances/ADV-GATEWAY-001.md, ADV-CONSOLE-001.md: TD-009
  connection constraints
- arrive/.../components/store.yaml: TD-011 connection-identity invariant
- docs/cloud-agent-notes.md: docs/tech-direction/ added to the binding
  reading list
- arrive/implementation-plan.yaml: ADV-STORE-007 item added (done)

## Check for Understanding

1. EMB-001 showed the hashing baseline ranking the paraphrased scope-isolation
   query second. What did BGE-small-en-v1.5 do with that same query, and why
   does the test *also* re-run the hashing baseline in the same test body
   rather than trusting the older test?
2. ADV-STORE-001 will define `DEFINE INDEX ... HNSW DIMENSION 384 DIST
   COSINE` before any embedding pipeline exists. Which executed assertion in
   `local_model_quality.rs` does that 384 rest on, and what two things must
   change together if the model is ever swapped?
3. Why is the `local-model` feature off by default, what happens if a cloud
   sandbox run enables it, and which two earlier advances established this
   same environment-split pattern?
4. The measured construction time is ~276 s cold but ~124 ms warm. What does
   ADV-STORE-002's spec now require services to do because of this, and what
   does a sandboxed deployment image need?
5. Which invariant did TD-011 add to `store.yaml`, and what silent failure
   mode does it protect the write path from?
