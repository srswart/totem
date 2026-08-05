---
advance:
  id: "ADV-STORE-002"
  title: "Embedding pipeline + vector index (gap-fill)"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store", "gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 40
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["new_dependency"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
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
- Saved memories get an embedding (synchronously or via a backfill worker,
  per ADV-STORE-003's placement decision) and `totem_recall` performs real
  vector similarity search combined with graph traversal in one SurrealQL
  round trip.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: embedding-attachment and similarity-retrieval tests (red first)
- [ ] feat: embedding generation + vector index + recall integration

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: provider choice affects cost, latency, and offline behavior; changing
  embedding models later invalidates the index (plan for re-embedding).
- Rollback: recall falls back to graph + keyword relevance without the vector
  term; index can be dropped and rebuilt.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-STORE-002 --status passed`

## Changes Made

- None yet
