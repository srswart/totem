---
advance:
  id: "ADV-STORE-005"
  title: "Enablement: synthetic memory corpus (test data for evaluations)"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store", "core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: enablement
  facets: [software, quality, security]
  work_products: [test_data]
  status: planned
---

## Objective

Build the synthetic memory corpus that the evaluation advances
(ADV-CORE-005 quality, ADV-GATEWAY-006 security, ADV-GATEWAY-007 performance)
run against: a seeded memory estate spanning all six categories and all four
scopes, with golden recall queries and expected results, plus adversarial
fixtures (near-duplicates, contradictions, cross-scope lookalikes designed to
surface scope leaks).

## Outcome

After this advance:
- One command seeds a reproducible corpus into a fresh store: multiple actors,
  projects, and a team/platform layer; per-category lifecycle variety (aged
  Knowledge, expired Context, contested Uncertainty pairs).
- Golden query set with expected ranked results exists for recall-quality
  scoring; leak-bait fixtures exist for security evaluation.
- Reset/cleanup is deterministic so evaluations are repeatable.

## Planned Work

- [ ] branch: create or confirm feature branch for this advance
- [ ] define required scenarios with the evaluation advances' needs as input
- [ ] feat: corpus generator + seed/reset commands
- [ ] golden queries + expected results checked in beside the generator

## Bug Fixes

- [ ] None yet

## Required Scenarios

- All six categories × all four scopes, incl. same-content memories at
  different scopes (leak bait) and near-duplicate Knowledge (dedupe input).
- Aged/decayed memories for currency scoring; contested pairs for Uncertainty.
- Multiple actors/projects to exercise scope-chain merge and precedence.

## Provenance and Privacy

- Synthetic data only — no real session content, names, or repo data. Every
  record is generator-tagged so a synthetic memory can never be mistaken for
  a real one if a corpus leaks into a shared instance.

## Setup, Reset, and Cleanup

- Seed and reset are idempotent CLI/test-harness commands against an embedded
  or dedicated test database; never run against a shared deployment.

## Risk + Rollback

- Risk: corpus not representative of real memory shapes — refresh it from
  anonymized patterns once real usage exists.
- Rollback: drop the test database; generator is deterministic.

## Evidence

- [ ] profile:selected-practices
- [ ] enablement:artifact

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-STORE-005 --status passed`

## Changes Made

- None yet
