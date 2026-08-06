---
advance:
  id: "ADV-CORE-006"
  title: "Auth refusals join the access log"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store", "gateway"]
  started_at: "2026-08-06T13:45:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 25
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

Close the audit gap ADV-GATEWAY-003 disclosed as a partially established
invariant: refused requests append nothing to the access log. The gateway
component's "no unlogged access" invariant currently covers every *successful*
read and write; a credential probing the boundary — wrong repo, expired
token, scope it doesn't own — is precisely the event a security audit wants
to see, and today it leaves no trace.

Wired as a dependency of ADV-GATEWAY-006 (the security evaluation must find
refusals in the log, not flag their absence).

## Behavioral Change

After this advance:

- `AccessOperation` (totem-core) gains a refusal variant carrying what was
  presented and why it was refused (the `AuthError` kind, the route, the
  fingerprint of the presented credential if any — never the token text).
- The gateway's auth layer appends a refusal entry for every request it
  turns away, on every route, including the MCP surface — error paths, not
  just the happy path, per the component hazard note.
- Refusal entries are queryable alongside the rest of the access log and are
  append-only like everything else in it.
- A test proves a refused request leaves exactly one refusal entry, and a
  control proves the entry appears even when the store itself is never
  reached (the refusal happens above the store — the log write is the only
  store touch).

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: refusal entries per route (REST + MCP), no-token and bad-token
      shapes, append-only holds for refusal entries
- [ ] feat: the `AccessOperation` variant, store write path, gateway wiring

## Bug Fixes

- [ ] None yet

## Scope and Boundaries

**In scope:** the refusal variant, its store write, gateway wiring on all
routes, tests.

**Out of scope:** rate limiting or lockout on repeated refusals (a later
curator/infra concern); repo-binding itself (ADV-GATEWAY-009); console
surfacing of refusal entries (belongs with ADV-CONSOLE-002's audit views).

## Risk + Rollback

- Risk (`auth` flag): the refusal path must not become a write amplifier —
  an unauthenticated request now causes a store write. Keep the entry
  minimal, and note (not solve) the flood concern for the rate-limiting
  follow-up.
- Risk: logging must never turn a refusal into a success — the log write
  failing must not flip the response, and the refusal must stand even if
  the entry cannot be appended (refuse first, log best-effort, count the
  discrepancy plainly in the advance if one is possible).
- Rollback: revert branch; refusals return to unlogged, the recorded
  residual state.

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
