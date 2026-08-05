---
advance:
  id: "ADV-CONSOLE-002"
  title: "Audit trails, Uncertainty queue, promotion approvals"
  system: "058-totem-core"
  primary_component: "console"
  components: ["console", "gateway"]
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
  status: planned
---

## Objective

Governance surfaces in the console (Solution Intent §5): audit trail viewer
(any memory's provenance and access history on demand — G3/success measure
"auditability"), the contested-memory (Uncertainty) queue with resolution
recording, promotion approvals for human-gated scope promotions, and the
value/usage reports incl. the "retire?" queue fed by ADV-CORE-002.

## Behavioral Change

After this advance:
- A reviewer can reconstruct any memory's lineage (derived-from episodes,
  curator actions, accesses) from the console.
- Contested memories sit in a queue until a human resolves them; the
  resolution is recorded, not silently applied.
- Human-gated promotions (e.g. Instructions to project/platform scope) are
  approved or rejected in the console, and the decision is a recorded event.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: API-contract tests for audit/queue/approval endpoints; view tests where supported
- [ ] feat: audit viewer, Uncertainty queue, promotion approval views + gateway endpoints

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: approval actions mutate governance state; wrong wiring could
  auto-approve gated promotions. Depends on the promotion engine
  (ADV-CORE-003) being in place.
- Rollback: revert branch; promotion decisions are recorded events and can be
  compensated with a demotion event.

## Evidence

- [ ] tidy:preparatory
- [ ] tests:unit
- [ ] tests:e2e

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CONSOLE-002 --status passed`

## Changes Made

- None yet
