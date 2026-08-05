---
advance:
  id: "ADV-CURATOR-001"
  title: "First curator job (dedupe) with supersede/rollback"
  system: "058-totem-core"
  primary_component: "curator"
  components: ["curator", "core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["concurrency"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

First AI curation job: deduplication of near-duplicate Knowledge memories,
running as a background agent against the same core API (Solution Intent §5).
Establishes the curator framework and its non-negotiable action model:
supersede + log + reversible — never delete.

## Behavioral Change

After this advance:
- The dedupe job identifies near-duplicate Knowledge memories (vector
  similarity + graph context) and merges them by writing a superseding record;
  originals remain, marked superseded, with lineage links.
- Every curator action is logged and reversible: a rollback restores the
  superseded originals and retires the merge.
- The job runs idempotently and safely alongside live writes.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: supersede/rollback invariant tests — no destructive path exists (red first)
- [ ] feat: curator job runner + dedupe job

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: "curation trust" is a named key risk — silent rewriting would destroy
  auditability. Mitigated by the append-only episodic substrate and the
  supersede-only action model; concurrency with live writes needs care.
- Rollback: pause the curator; run the built-in rollback for any bad merges.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CURATOR-001 --status passed`

## Changes Made

- None yet
