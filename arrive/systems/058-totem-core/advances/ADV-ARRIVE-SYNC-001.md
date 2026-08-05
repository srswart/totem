---
advance:
  id: "ADV-ARRIVE-SYNC-001"
  title: "Ingest this repo's own /arrive/ into the landscape graph (dogfood)"
  system: "058-totem-core"
  primary_component: "arrive-sync"
  components: ["arrive-sync", "store"]
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
  status: planned
---

## Objective

First landscape ingestion: parse this repo's own `/arrive/` artifacts
(registry, system, components, advances) into graph entities
(`repo` → `system` → `component`, `advance` with status, `impacts` /
`depends_on` / `owned_by` edges) with sync provenance recorded
(Solution Intent §2.3).

## Behavioral Change

After this advance:
- Running the sync against 058-totem populates the landscape graph;
  `totem_landscape` / the REST landscape query answers "what is this project
  made of, what's in flight, what just finished" in one round trip (G2).
- Re-running the sync is idempotent and records provenance for each ingestion.
- `/arrive/` files remain authoritative — sync is read-only toward the repo.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: parse + graph-mapping tests using this repo's artifacts as fixtures (red first)
- [ ] feat: artifact parser, graph writer, sync provenance record

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: mapping drift between ARRIVE artifact schema versions and the graph
  model; mitigated by dogfooding on this repo first.
- Rollback: revert branch; landscape entities can be re-derived from `/arrive/`
  at any time (the mirror is disposable, the repo is authoritative).

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-ARRIVE-SYNC-001 --status passed`

## Changes Made

- None yet
