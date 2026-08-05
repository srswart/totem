---
advance:
  id: "ADV-STORE-003"
  title: "Investigation: embedding provider + placement spike"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software]
  work_products: []
  status: planned
---

## Objective

Experiment to close the Solution Intent §9 open question: embedding provider
(local model vs. API) and placement (gateway on write vs. curator batch).
Compare candidates on retrieval quality over memory-sized texts, latency,
cost, and offline behavior, using throwaway spike code.

## Outcome

After this advance:
- A recorded recommendation (provider + placement + re-embedding strategy)
  with the comparison data behind it, written up as a `docs/tech-direction/`
  entry and referenced by ADV-STORE-002, which implements it.
- No production code lands; spike code is disposable and clearly marked.

## Planned Work

- [ ] branch: create or confirm feature branch for this advance
- [ ] define comparison criteria (quality on memory-sized texts, p50/p95 latency, cost, offline)
- [ ] spike: run 2–3 candidate providers against sample memory texts
- [ ] write findings + recommendation to docs/tech-direction/embeddings.md

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: sample texts may not represent real memory content; revisit after
  real usage data exists.
- Rollback: n/a — findings only, no production changes.

## Evidence

- [ ] profile:selected-practices
- [ ] investigation:findings

## CI Evidence Notes

- Findings document is the primary artifact; no pipeline evidence expected.

## Changes Made

- None yet
