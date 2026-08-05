---
advance:
  id: "ADV-CORE-003"
  title: "Scope promotion policy engine (gap-fill)"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 40
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

**Gap-fill** (see docs/arrive-decomposition-gaps.md): "sharing is by
promotion" is a pillar of the model (Solution Intent §2.2), and ADV-CONSOLE-002
builds the approval UI — but no advance builds the promotion mechanics.
Implement propose/approve/reject/demote as recorded events with per-category
policy: auto-approval for low-risk categories (e.g. Knowledge), human-gated
for Instructions.

## Behavioral Change

After this advance:
- A memory at `actor` scope can be proposed for `project`/`team`/`platform`
  scope; the proposal, decision, and effective scope change are recorded
  events with full provenance.
- Policy decides the path per category: auto-promote where allowed, queue for
  human approval where gated (the queue ADV-CONSOLE-002 renders).
- Demotion exists as the compensating event; no promotion is ever a silent
  in-place scope edit.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: policy-path tests per category, incl. rejection + demotion (red first)
- [ ] feat: promotion events, policy engine, store enforcement

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: promotion is the one sanctioned path across scope boundaries — a
  policy bug is a scope-leak vector (the highest-severity failure class).
- Rollback: demotion events compensate any bad promotion; policy can be
  tightened to human-gated-for-everything via config.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CORE-003 --status passed`

## Changes Made

- None yet
