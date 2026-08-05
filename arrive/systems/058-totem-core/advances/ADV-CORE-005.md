---
advance:
  id: "ADV-CORE-005"
  title: "Quality evaluation: recall relevance + ranking behavior"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store", "gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 35
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: evaluation
  facets: [quality]
  work_products: []
  status: planned
---

## Objective

Evaluate recall quality once the value loop is wired (after ADV-CORE-002 +
ADV-STORE-002): does retrieval actually favor relevant, valuable, current
memory? Runs the recall-quality scorer (ADV-GATEWAY-008) over the golden
corpus (ADV-STORE-005) and dispositions findings.

## Outcome

After this advance:
- Measured ranking quality against the golden set, per category, with the
  value × currency multipliers on and off — proving the value loop improves
  (not degrades) retrieval.
- Findings dispositioned: fixes filed as advances, accepted limitations
  recorded as residual risk.

## Planned Work

- [ ] branch: create or confirm feature branch for this advance
- [ ] run scorer over golden corpus: baseline (relevance only) vs. full ranking
- [ ] probe category weighting (Instructions priority, Context TTL, decayed Knowledge sinking)
- [ ] disposition findings; file follow-up advances for defects

## Bug Fixes

- [ ] None yet

## Quality Risks

- Ranking regressions from the value/currency multipliers; category weights
  inverting intent (e.g. stale Instructions outranking fresh Context);
  scope-chain merge dropping or duplicating relevant items.

## Test Scope and Method

- Golden-query scoring (ADV-GATEWAY-008 scorer) on the seeded corpus
  (ADV-STORE-005); A/B of ranking formula terms; per-category breakdown.
  Out of scope: live-session reuse-rate measurement (needs production data).

## Coverage

- All six categories, all four scopes, aged and contested fixtures; both MCP
  and REST recall paths.

## Findings and Disposition

- To be completed at evaluation time; every finding gets a disposition
  (fixed / accepted / deferred with trigger).

## Residual Risk

- Golden-corpus results are a proxy; the real metric is the brief's
  reuse-rate success measure, which needs production sessions to compute.

## Risk + Rollback

- Risk: evaluation is read-only against a seeded instance.
- Rollback: n/a — findings only.

## Evidence

- [ ] profile:selected-practices
- [ ] profile:honest-practices
- [ ] analysis:artifact

## CI Evidence Notes

- Attach scorer reports as the analysis artifact.

## Changes Made

- None yet
