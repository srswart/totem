---
advance:
  id: "ADV-GATEWAY-003"
  title: "Streamable-HTTP MCP + auth for cloud agents"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth", "public_api"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Serve MCP over streamable HTTP with token auth so cloud agents (Cursor
background agents, Anthropic cloud sessions, CI-driven agents) can attach
(Solution Intent §3.1). Tokens are least-privilege: bound to repo + scope
(gateway invariant). Verify per-harness remote-MCP support at build time
(open question in §9).

## Behavioral Change

After this advance:
- A cloud agent holding a scoped token can call the same tool surface as
  desktop harnesses; a token bound to repo A + actor scope cannot read repo B
  or another actor's memories — covered by authorization tests.
- Token issuance/revocation exists for admin use (console/CLI wiring may land
  separately).

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: authn/authz tests — token scope bounds, expiry, revocation (red first)
- [ ] feat: streamable-HTTP MCP transport + token auth layer

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: auth flaw here is a scope-leak vector (the highest-severity failure
  class); treat authorization tests as blocking evidence.
- Rollback: disable the HTTP MCP listener (config), leaving stdio unaffected;
  revoke issued tokens.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-GATEWAY-003 --status passed`

## Changes Made

- None yet
