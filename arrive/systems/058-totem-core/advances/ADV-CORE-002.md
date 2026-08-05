---
advance:
  id: "ADV-CORE-002"
  title: "Metering + value/currency scoring in retrieval ranking"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Implement the value loop (Solution Intent §4): per-memory counters
(retrievals, injections, citations), per-actor/per-harness usage aggregates,
`value_score` updated from explicit feedback and implicit use signals, and a
`currency` term that decays with time and refreshes on reinforcement.
Retrieval ranking becomes relevance (vector + graph proximity) × value ×
currency, weighted per category.

## Behavioral Change

After this advance:
- Recall results are ranked by the combined score; category weights are
  configurable (e.g. Instructions get highest injection priority).
- Feedback signals (used / wrong / stale) move `value_score`; time moves
  `currency` down and reinforcement refreshes it.
- Low-value, low-currency memories sink in ranking and become eligible for the
  console's "retire?" queue instead of disappearing silently (G4).

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: scoring/decay/ranking property tests (red first)
- [ ] feat: counters, score updates, ranking function wired into recall

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: value attribution is the brief's acknowledged hard problem — start
  with simple proxies (retrieval → citation → outcome) and iterate; a bad
  ranking function degrades recall quality for every harness.
- Rollback: feature-flag the ranking multiplier back to pure relevance;
  counters are additive data.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CORE-002 --status passed`

## Changes Made

- None yet
