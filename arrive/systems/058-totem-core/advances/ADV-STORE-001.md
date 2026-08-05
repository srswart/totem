---
advance:
  id: "ADV-STORE-001"
  title: "SurrealDB schema + repositories + scope-isolation tests"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store", "core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["migration", "new_dependency"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Define the SurrealDB schema for typed memory and the landscape graph, a
migration mechanism, and repository APIs in `totem-store` — with scope
isolation enforced at the store layer as an invariant (Solution Intent §2.2,
Project Brief "Key risks": leaking private context across scopes is the
highest-severity failure).

## Behavioral Change

After this advance:
- Memory records persist and load through repositories (embedded SurrealDB for
  dev/tests) with all frontmatter groups: identity, content, provenance,
  economics, governance.
- Scope-chain reads (actor → project → team → platform) resolve with dedup and
  precedence into one merged view.
- Tests prove the store invariants: a caller scoped to one actor cannot read
  another actor's memories; episodic records reject updates/deletes; writes
  without provenance are rejected.

## Constraints from completed investigations (binding)

Read [docs/tech-direction/surrealdb.md](../../../../docs/tech-direction/surrealdb.md)
§4 before writing any schema — its constraints (TD-002 knn syntax with EXPLAIN
assertions, TD-004 datetime binding with a regression test, TD-005, TD-008) are
requirements of this advance, not background. Additionally:

- **Vector index pin:** `DIMENSION 384 DIST COSINE` — derived from the
  measured embedding decision in
  [docs/tech-direction/embeddings.md](../../../../docs/tech-direction/embeddings.md)
  (EMB-004: BGE-small-en-v1.5 via fastembed). The embedding *pipeline* lands in
  ADV-STORE-002; the schema fixes the dimension now so no index migration is
  needed between the two.
- **Connection identity (TD-011):** the store connects as a fully-privileged
  system user and enforces ALL authorization itself. Restricted DB roles are
  unusable — their data writes are silently discarded, and roles are not row
  filters. This is now a component invariant on `store`.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: scope-isolation, append-only-episodic, and provenance-required tests (red first)
- [ ] test: TD-004 regression (string-bound cutoff must be impossible to express) and TD-002 EXPLAIN assertion
- [ ] feat: schema (HNSW DIMENSION 384 DIST COSINE) + migrations + repositories to pass tests

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: scope-isolation bug leaks private context — highest-severity failure in
  the brief. Schema decisions here are the hardest to migrate later.
- Rollback: revert branch; drop dev database. No production data in v1 at this point.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-STORE-001 --status passed`

## Changes Made

- None yet
