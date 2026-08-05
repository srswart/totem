---
advance:
  id: "ADV-GATEWAY-006"
  title: "Security evaluation: scope isolation, auth, promotion boundaries"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "store", "core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: evaluation
  facets: [security]
  work_products: []
  status: planned
---

## Objective

Adversarial evaluation of the highest-severity failure class in the brief:
leaking private context across scopes. Runs after the auth surface exists
(ADV-GATEWAY-003) and the promotion engine lands (ADV-CORE-003), against the
leak-bait corpus (ADV-STORE-005).

## Outcome

After this advance:
- A documented attempt to breach every scope boundary through every surface
  (REST, stdio MCP, HTTP MCP, live queries), with findings dispositioned by
  severity and fixes filed as advances.
- Confirmation that the store-level invariant holds even when the gateway is
  misused — or concrete evidence of where it doesn't.

## Planned Work

- [ ] branch: create or confirm feature branch for this advance
- [ ] threat model workshop output checked in (assets, actors, boundaries)
- [ ] adversarial test pass per surface using leak-bait fixtures
- [ ] SAST/SCA (dependency audit) run and triaged
- [ ] disposition findings; file remediation advances

## Bug Fixes

- [ ] None yet

## Security Scope and Boundaries

- In scope: scope-chain read resolution, token scope binding (repo + scope),
  promotion propose/approve/demote paths, access-log completeness (can an
  access avoid being logged?), curator's cross-scope read behavior, live-query
  subscriptions. Out of scope: console human SSO (not yet built, tracked in
  gaps doc), infrastructure hardening, DoS resilience.

## Threat Model or Risk Hypothesis

- A malicious or misconfigured agent holding a valid low-privilege token
  attempts to read or write outside its actor/project scope — via crafted
  recall queries, promotion abuse, landscape queries over other repos, or
  live-query subscription to scopes it cannot read. Secondary: a curator bug
  moves content across scopes during merge.

## Controls and Test Method

- Controls exercised: store-layer scope filters, token authz middleware,
  promotion policy gates, access logging. Method: adversarial integration
  tests with leak-bait fixtures + focused code review of the scope-filter and
  authz paths + dependency SCA. Findings narrative required — a clean tool
  run alone is not a pass.

## Findings and Disposition

- To be completed at evaluation time; every finding carries severity and
  disposition (fixed / accepted / deferred with trigger).

## Residual Risk

- To be recorded at evaluation time (e.g. surfaces not yet built, deferred
  hardening).

## Risk + Rollback

- Risk: evaluation runs against a seeded test instance, never shared data.
- Rollback: n/a — findings only; fixes land as their own advances.

## Evidence

- [ ] profile:selected-practices
- [ ] profile:honest-practices
- [ ] security:scope-documented
- [ ] security:findings-dispositioned

## CI Evidence Notes

- Attach SAST/SCA reports and adversarial test output as evidence references,
  alongside — not instead of — the findings narrative.

## Changes Made

- None yet
