---
advance:
  id: "ADV-STORE-004"
  title: "Investigation: SurrealDB multi-model one-round-trip spike"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store"]
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
  facets: [software]
  work_products: []
  status: planned
---

## Objective

Validate the load-bearing assumption behind G2 and the read→think→write turn
model (Solution Intent §1): that one SurrealQL round trip can assemble graph
traversal + vector search + temporal facts into complete context, and that
decisions, entity updates, and triggered events commit in one ACID
transaction. Also exercise live queries (console feed) and the embedded vs.
server deployment modes.

## Outcome

After this advance:
- A spike proves (or refutes, with fallback options) the one-round-trip
  recall query and the single-transaction write path on a realistic toy
  schema; findings recorded, with any SurrealDB version constraints and
  feature caveats noted for ADV-STORE-001 to build on.
- If refuted, a documented fallback (e.g. two coordinated queries) and its
  cost, before schema work begins.

## Planned Work

- [ ] branch: create or confirm feature branch for this advance
- [ ] spike: toy schema with document + graph + vector + temporal data
- [ ] spike: one-round-trip recall query; multi-write ACID transaction; live query
- [ ] verify engine parity: every capability above (esp. live queries and vector
      indexes) behaves the same on the embedded in-memory engine (`kv-mem`,
      what all tests use) as on server mode (what production will use);
      document any capability that only works against a server
- [ ] write findings incl. version/feature caveats to docs/tech-direction/surrealdb.md

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: discovering the assumption fails late would invalidate the schema
  design — which is exactly why this runs before ADV-STORE-001 completes.
- Rollback: n/a — findings only.

## Evidence

- [ ] profile:selected-practices
- [ ] investigation:findings

## CI Evidence Notes

- Findings document is the primary artifact; no pipeline evidence expected.

## Changes Made

- None yet
