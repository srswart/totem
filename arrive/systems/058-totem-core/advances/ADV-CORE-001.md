---
advance:
  id: "ADV-CORE-001"
  title: "Workspace scaffold + domain types (memory categories, scopes, provenance)"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["new_dependency"]
  evidence: []
  # Populated by `arrive usage import` / the LiteLLM callback — leave empty when authoring.
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Stand up the Rust workspace (`crates/totem-core`, with sibling crate slots per
Solution Intent §7) and define the core domain types: the six memory categories
(Episodic, Identity, Knowledge, Context, Instructions, Uncertainty), the scope
enum (`actor` / `project` / `team` / `platform`), the common memory record shape
(identity, content, provenance, economics, governance groups), and provenance
types. See docs/solution-intent.md §2.

## Behavioral Change

After this advance:
- `cargo build` succeeds on a workspace containing `totem-core`.
- Domain types for memory categories, scopes, and provenance exist with
  serde round-trip support and unit tests; category determines lifecycle
  metadata (e.g. Episodic is append-only, Context carries a TTL).
- The record shape marked "variable" in the Solution Intent is firmed up here.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: move any `src/**` remnants into the workspace layout (no behavior change)
- [ ] test: category/scope/provenance type tests (serde round-trip, lifecycle rules)
- [ ] feat: workspace scaffold + `totem-core` domain types

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: domain shape decided here constrains store schema and gateway API;
  wrong category/scope modeling is expensive to change later.
- Rollback: revert the advance branch; no persisted data exists yet.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit

## CI Evidence Notes

- If CI jobs are enabled, link pipeline evidence (`ci:passed`) from PR/MR and default-branch runs.
- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CORE-001 --status passed`

## Changes Made

- None yet
