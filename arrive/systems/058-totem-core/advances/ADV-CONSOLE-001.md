---
advance:
  id: "ADV-CONSOLE-001"
  title: "Landscape dashboard + memory browser (read-only)"
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

First Dioxus console (web target): read-only landscape dashboard
(systems/components/advances across enrolled repos) and memory browser filtered
by scope and category (Solution Intent §5). Console updates ride SurrealDB
live queries through the gateway.

## Behavioral Change

After this advance:
- A developer can open the console, see the landscape of enrolled repos, and
  browse memories by scope and category — the first "humans observe" surface (G5).
- Landscape changes (e.g. an advance status flip after sync) appear without a
  manual refresh via live-query updates.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: component/view tests where the Dioxus toolchain supports them; API-contract tests otherwise
- [ ] feat: Dioxus app shell, landscape dashboard, memory browser

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: Dioxus web toolchain maturity; keep views thin over the REST API so a
  frontend pivot would not touch the backend.
- Rollback: revert branch; console is read-only, no data at risk.

## Evidence

- [ ] tidy:preparatory
- [ ] tests:unit
- [ ] tests:e2e

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CONSOLE-001 --status passed`

## Changes Made

- None yet
