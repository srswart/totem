---
advance:
  id: "ADV-STORE-002"
  title: "Embedding pipeline + vector index (gap-fill)"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store", "gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-05T19:27:55Z"
  review_time_estimate_minutes: 77
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 38
  risk_flags: ["new_dependency", "concurrency"]
  evidence: ["tdd:red-green", "tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

**Gap-fill** (see docs/arrive-decomposition-gaps.md): the record shape includes
an `embedding` vector and recall ranking depends on vector search, but no
roadmap advance produces embeddings. Implement embedding generation per the
**measured** decision in
[docs/tech-direction/embeddings.md](../../../../docs/tech-direction/embeddings.md)
(ADV-STORE-003 + ADV-STORE-007 / EMB-004): **BGE-small-en-v1.5 via `fastembed`,
384 dimensions, embedded synchronously at gateway-write time**; the curator
owns re-embedding on model-version change. The HNSW index (DIMENSION 384) is
ADV-STORE-001's; this advance fills it.

**Environment constraint (EMB-002/EMB-003, binding on the implementing
agent):** the cloud sandbox cannot download model weights. Structure the work
as: an `Embedder` trait in production code; sandbox tests run against a
deterministic test embedder (the spike's hashing shape is fine for tests);
real `fastembed` inference behind an off-by-default cargo feature verified on
a workstation (the ADV-STORE-006/007 split). Load the model once at service
startup — cold construction is ~276 s (download) vs ~124 ms warm; a sandboxed
deployment must bake the weights into its image. Do not mark the model-backed
path as sandbox-verified — say plainly which half ran where.

## Behavioral Change

After this advance:
- Memory content can be embedded via `totem_store::embed`, an `Embedder`
  trait, and a store-level pipeline: text in, a validated 384-dimension
  vector out, attached to `Content`. `RecallQuery::near` (ADV-STORE-001) then
  ranks those *generated* embeddings by real vector similarity through the
  HNSW index, proven end-to-end (attach → save → recall) rather than only
  against hand-crafted probe vectors.
- **Correction to this advance's original wording:** the line above no longer
  claims recall combines vector search "with graph traversal in one SurrealQL
  round trip" as new behavior of this advance. That capability describes
  TD-001's finding about what SurrealDB *can* do in one round trip; ADV-STORE-001
  did not project a `->derived_from->` graph traversal into the recall
  statement, and this advance does not add one either — recall still selects
  `*` from `memory` with the scope/category/temporal/vector predicates. Wiring
  graph traversal into recall's projection is unclaimed, future work (no
  advance currently owns it).
- Placement is **not** wired into the store's `save` path here: the tech
  direction calls for gateway-on-write, and `totem-gateway` does not exist yet
  (ADV-GATEWAY-001). `embed` is a plain function over `Content`, callable
  before `save`, so a future gateway (or this advance's own tests) can attach
  an embedding without the store crate depending on HTTP or MCP.
- Curator-owned re-embedding on model-version change (mentioned in the
  Objective) is not implemented: `totem-curator` does not exist yet
  (ADV-CURATOR-001). Recorded here as explicitly out of scope, not silently
  dropped.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change) — **skipped**: no
      existing code needed refactoring before this addition; the module is
      wholly new.
- [x] test: embedding-attachment and similarity-retrieval tests (red first)
- [x] feat: embedding generation + vector index + recall integration (the
      vector index itself is ADV-STORE-001's; this advance is the generation
      + attachment pipeline that fills it)

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: provider choice affects cost, latency, and offline behavior; changing
  embedding models later invalidates the index (plan for re-embedding —
  unimplemented, see Behavioral Change).
- Risk (new): `FastembedEmbedder` wraps its model session in a `Mutex` because
  `fastembed`'s `embed` takes `&mut self` while `Embedder` is a
  shared-reference trait; under concurrent load every embed call serializes
  on that lock. Untested here — the `fastembed` feature is off by default and
  this sandbox cannot exercise it (EMB-002/EMB-003) — so treat single-threaded
  throughput as the only measured number until a workstation run says
  otherwise.
- Rollback: recall falls back to keyword/category/temporal relevance without
  the vector term (unchanged from ADV-STORE-001); the new `embedding` module
  is additive and can be reverted independently of the schema or index.

## Evidence

- [x] tidy:preparatory — **not applicable**: no tidy commit; nothing existed
      to tidy (see Planned Implementation Tasks).
- [x] tdd:red-green — `tests/embedding.rs` was written against symbols that
      did not exist yet; verified via `git stash` (isolating the test file
      from the implementation) that `cargo test -p totem-store --test
      embedding` failed with `E0432: unresolved imports` (the right reason —
      not a typo or an unrelated compile error) before the implementation was
      restored and the same run went green (6/6).
- [x] tests:unit — dimension validation, determinism, and body/tag
      preservation (`tests/embedding.rs`, 5 of its 6 tests).
- [x] tests:integration — `recall_ranks_generated_embeddings_by_similarity`
      exercises attach → save → recall end-to-end against the real embedded
      SurrealDB engine.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-STORE-002 --status passed`

## Reviewability

`arrive score --base origin/advance/phase-003` reports **38 [YELLOW]** (size
22, novelty 11, risk 5 — the `concurrency` flag is the `Mutex` in
`FastembedEmbedder`, noted above). Not split further: the change is one
cohesive unit (an `Embedder` trait, one production implementation gated
off by default, one sandbox test double, and the tests proving the pipeline)
and the commit series already separates test-first from implementation per
the tidy→test→feat convention.

## Changes Made

### 2026-08-05 - test: embedding-attachment and similarity-retrieval tests (red first)
- crates/totem-store/tests/embedding.rs: added, referencing `DeterministicEmbedder`,
  `Embedder`, and `embed` before they existed in `totem-store`'s public API —
  confirmed to fail to compile (`E0432`) via `git stash` before the
  implementation landed.

### 2026-08-05 - feat: embedding generation pipeline
- crates/totem-store/src/embedding.rs: added — the `Embedder` trait, the
  `embed` function (generates and dimension-checks a vector, attaching it to
  `Content`), and `DeterministicEmbedder` (a deterministic, offline,
  non-semantic stand-in — the trigram-hashing shape ADV-STORE-003's spike
  measured as EMB-001 — used by this crate's own tests).
- crates/totem-store/src/embedding/fastembed_embedder.rs: added, behind the
  new `fastembed` cargo feature — `FastembedEmbedder`, loading BGE-small-en-v1.5
  via `fastembed` once at construction (EMB-004's recommended production
  model). Off by default; not exercised in this sandbox (EMB-002/EMB-003) and
  not covered by any test run here.
- crates/totem-store/src/lib.rs: exported `Embedder`, `DeterministicEmbedder`,
  `embed`, and (feature-gated) `FastembedEmbedder`.
- crates/totem-store/Cargo.toml: added `fastembed = "=5.17.4"` as an optional
  dependency behind the `fastembed` feature, pinned to the same version
  ADV-STORE-007 measured.
- crates/totem-store/tests/embedding.rs: (from the test commit above) now
  passes — 6/6 green, including the end-to-end recall-by-similarity test.
- Cargo.lock: updated for the new optional dependency.

## Check for Understanding

1. Why does `embed` live as a free function over `totem_core::Content` rather
   than a method on `Store` or `MemoryRepository`? What would change if
   `totem-gateway` (ADV-GATEWAY-001) existed today and needed to call it?
2. `DeterministicEmbedder` and `FastembedEmbedder` both implement `Embedder`,
   but only one is exercised by this sandbox's test run. Which, and why —
   what specifically about this environment makes the other one unverifiable
   here, and where is that documented?
3. `tests/embedding.rs::recall_ranks_generated_embeddings_by_similarity`
   asserts the lexically-closer record ranks first. Given
   `DeterministicEmbedder` is a trigram-hashing, non-semantic embedder (not
   BGE-small-en-v1.5), what would happen to this test's assertion if the two
   memory bodies shared no vocabulary with the probe query at all — and does
   that limitation matter for what this test is actually trying to prove?
4. The Behavioral Change section above corrects a claim from this advance's
   original draft about graph traversal. What does `RecallQuery` actually
   select today, and what would need to change — and in which file — for a
   recall to also traverse `provenance.derived_from` links?
5. `embed` rejects an embedder whose output has the wrong length before the
   result reaches `MemoryRepository::save`. Trace both places a dimension
   mismatch is checked in this crate now (`src/embedding.rs` and
   `src/memory.rs`) — why is the check duplicated instead of relying on only
   one of them?
