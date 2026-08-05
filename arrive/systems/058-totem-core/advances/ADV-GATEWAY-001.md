---
advance:
  id: "ADV-GATEWAY-001"
  title: "Axum REST API: recall/save with provenance + access log"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 40
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

Expose the first HTTP surface in `totem-gateway`: recall (merged, ranked
context across the scope chain) and save (write with provenance auto-attached),
with every read and write appended to the access log (Solution Intent §3.2, §4).

## Behavioral Change

After this advance:
- `POST /recall` returns merged, scope-resolved context for a query;
  `POST /save` persists a memory with provenance attached from the caller's
  session identity.
- Every request appends an access-log record (who, what, when, via which
  endpoint, in which session) — verifiable by an audit query.
- The gateway stays thin over the core API (mitigation for the "standard
  drift" risk in the brief).

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: endpoint tests incl. access-log assertions (red first)
- [ ] feat: Axum router, recall/save handlers, access-log middleware

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: API shapes become de-facto contracts for MCP tools and console; access
  logging gaps would undermine G3 (auditability).
- Rollback: revert branch; endpoints are additive, no schema change beyond the
  access-log table introduced with it.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-GATEWAY-001 --status passed`

## Changes Made

- None yet
