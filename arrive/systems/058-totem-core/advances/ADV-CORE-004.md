---
advance:
  id: "ADV-CORE-004"
  title: "Investigation: value-attribution proxy experiment"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core"]
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
  facets: [software, quality]
  work_products: []
  status: planned
---

## Objective

Experiment to close the §9 open question "value attribution depth": how far
to chase "did this memory change the outcome?" in v1. The brief names this a
key risk ("usage metering is easy, value attribution is hard"). Evaluate
candidate proxies — retrieval, citation in the following turn, explicit
feedback, outcome linkage — on recorded sample sessions, and pick the v1
signal set and per-category weights for ADV-CORE-002.

## Outcome

After this advance:
- A recorded decision on which signals feed `value_score` in v1, with the
  observed precision of each proxy on sample sessions and known failure
  modes (e.g. citation without use, use without citation).
- Deferred signals listed with the trigger conditions for revisiting.

## Planned Work

- [ ] branch: create or confirm feature branch for this advance
- [ ] collect/construct sample sessions with known "memory actually mattered" labels
- [ ] score candidate proxies against the labels
- [ ] write findings + v1 signal set to docs/tech-direction/value-attribution.md

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: hand-labeled samples bias the proxy choice; treat the v1 set as
  provisional and re-run once real feedback data flows.
- Rollback: n/a — findings only.

## Evidence

- [ ] profile:selected-practices
- [ ] investigation:findings

## CI Evidence Notes

- Findings document is the primary artifact; no pipeline evidence expected.

## Changes Made

- None yet
