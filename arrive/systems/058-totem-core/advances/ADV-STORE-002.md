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
  status: planned
---

## Objective

**Gap-fill** (see docs/arrive-decomposition-gaps.md): the record shape includes
an `embedding` vector and recall ranking depends on vector search, but no
roadmap advance produces embeddings. Decide the embedding provider (local
model vs. API — open question, Solution Intent §9) and where embedding happens
(gateway on write vs. curator batch), then implement generation and the
SurrealDB vector index.

## Behavioral Change

After this advance:
- Saved memories get an embedding (synchronously or via a backfill worker —
  decided here) and `totem_recall` performs real vector similarity search
  combined with graph traversal in one SurrealQL round trip.
- The provider decision and its trade-offs are recorded (candidate for a
  docs/tech-direction/ entry).

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] decide: embedding provider + placement; record the decision
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
