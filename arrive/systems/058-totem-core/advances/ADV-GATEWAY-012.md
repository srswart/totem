---
advance:
  id: "ADV-GATEWAY-012"
  title: "Durable credential registry"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "store", "cli"]
  started_at: "2026-08-07T04:30:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 35
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth", "migration"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: planned
---

## Objective

Credentials must survive a gateway restart: `TokenRegistry` is an in-memory
map today, so every grant except the env-seeded bootstrap vanishes on
restart — unworkable for a shared instance that issues per-routine and
per-person credentials (docs/dogfood/plan.md §3.2).

Cloud-eligible: ordinary store-backed Rust with the established patterns.

## Behavioral Change

After this advance:

- Issued grants (fingerprint, repo, scope, actor, expiry, revocation state)
  persist in the durable store and are loaded into the registry on start-up;
  the env-seeded bootstrap credential still works and is registered durably
  on first boot.
- Issue/list/revoke exist as authenticated gateway endpoints (platform- or
  admin-bound credential required — issuing credentials is itself a
  privileged operation; define and test who may do it).
- Only fingerprints are ever stored — the plaintext-never-retained property
  extends to the durable rows, with a test proving a data-directory grep
  cannot find an issued token.
- Revocation takes effect immediately (registry and store together), and a
  restart cannot resurrect a revoked credential.
- A rotation runbook section in the advance: issue-new, verify, revoke-old.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: persistence across reopen, revocation permanence, fingerprint-only
      storage, issuance authorization refusals
- [ ] feat: store-backed registry + endpoints + bootstrap durability

## Risk + Rollback

- Risk (`auth`): the issuance endpoint is the new attack surface — an
  over-permissive issuer defeats every binding downstream. Issuance
  authorization gets the same adversarial test shape as ADV-GATEWAY-003.
- Risk (`migration`): first schema addition since DEP-001 made data durable;
  migrate against a populated directory.
- Rollback: revert branch; registry returns to in-memory + bootstrap.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
