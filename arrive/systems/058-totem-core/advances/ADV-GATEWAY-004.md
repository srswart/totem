---
advance:
  id: "ADV-GATEWAY-004"
  title: "MCP tools: feedback, contest, advance_status/advance_log (gap-fill)"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 35
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["public_api"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

**Gap-fill** (see docs/arrive-decomposition-gaps.md): four of the seven MCP
tools in Solution Intent §3.1 are not covered by any roadmap advance. Add
`totem_feedback` (explicit value signal: used / wrong / stale — the input side
of the value loop), `totem_contest` (file an Uncertainty record instead of
overwriting), and `totem_advance_status` / `totem_advance_log`
(process-attuned read/write for the active advance).

## Behavioral Change

After this advance:
- Agents can signal memory value explicitly; signals feed ADV-CORE-002's
  `value_score` updates.
- A contradiction becomes an Uncertainty record with both claims preserved —
  agents no longer have to overwrite or silently drop conflicting facts.
- An agent working an advance can read its status and append log entries
  through Totem, making sessions process-attuned to ARRIVE.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: tool tests incl. Uncertainty-record filing semantics (red first)
- [ ] feat: four MCP tools + backing REST endpoints

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: `totem_advance_log` writes about ARRIVE artifacts — writes go to
  Totem's mirror/memory only; `/arrive/` files in the repo stay authoritative
  (arrive-sync invariant).
- Rollback: revert branch; tools are additive to the existing surface.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-GATEWAY-004 --status passed`

## Changes Made

- None yet
