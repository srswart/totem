---
advance:
  id: "ADV-GATEWAY-009"
  title: "Repo-bind enroll and landscape reads; unify the repo id spaces"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "arrive-sync", "cli"]
  started_at: "2026-08-06T13:45:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: planned
---

## Objective

Close the enumeration vector ADV-GATEWAY-003 disclosed as a partially
established invariant: `/enroll` and `GET /landscape/:repo` are authenticated
but not repo-bound, so any valid credential can enroll or read the landscape
of **any** repo. Bind both routes to the credential's repo, which first
requires unifying the two repo id spaces that made the binding impossible to
express (`TokenGrant.repo` is an `owner/name` `RepoId`; the landscape sync
uses ARRIVE landscape ids like `058-totem`).

This must land before any second repo enrolls (the reserved
ADV-ARRIVE-SYNC-002 multi-repo sync), and before ADV-GATEWAY-006 evaluates
scope isolation — the evaluation must test the bound behavior, not grade the
hole (wired as a plan dependency).

## Behavioral Change

After this advance:

- One repo identity model, recorded in the advance: either the landscape
  snapshot carries the `owner/name` repo id, or credentials carry both ids —
  the decision and its migration for already-synced landscapes is part of the
  work, not an exercise for the reader.
- `POST /enroll` (REST and any MCP equivalent) refuses a snapshot whose repo
  identity is not the presenting credential's repo, with an `AuthError` that
  names both.
- `GET /landscape/:repo` and the `totem_landscape` MCP tool refuse a repo the
  credential is not bound to. No route can enumerate other repos' landscapes.
- Negative tests prove both refusals, plus a control proving the bound repo
  still round-trips; the ADV-GATEWAY-003 auth suite keeps passing unchanged.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: repo-mismatch refusals for enroll and landscape (REST + MCP),
      bound-repo control, id-space round-trip
- [ ] feat: id-space unification + repo binding on both routes

## Bug Fixes

- [ ] None yet

## Scope and Boundaries

**In scope:** the two unbound routes, the id-space unification they require,
and the migration of any existing landscape rows to the unified id.

**Out of scope:** auth-refusal logging (ADV-CORE-006); multi-repo sync
itself (reserved ADV-ARRIVE-SYNC-002); console human auth (see gaps doc,
now suggested as ADV-GATEWAY-010).

## Risk + Rollback

- Risk (`auth` flag): the binding must refuse — not filter. A landscape
  response that silently omits unauthorized repos would hide the violation
  from the caller and the audit trail alike; refuse loudly with the bound
  and requested ids, mirroring `AuthError::RepoMismatch`.
- Risk: id-space unification touches stored landscape rows; the migration
  runs under the store's ledger and is exercised against a directory with
  pre-unification data (DEP-001 makes old data real now).
- Rollback: revert branch; the routes return to authenticated-but-unbound,
  which is the currently recorded residual, not a regression.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7.

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation, grounded in the files actually
changed)
