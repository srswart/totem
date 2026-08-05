---
advance:
  id: "ADV-GATEWAY-008"
  title: "Enablement: evaluation harness (workload driver + recall-quality scorer)"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 35
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: enablement
  facets: [software, quality, performance]
  work_products: [performance_harness, test_automation]
  status: planned
---

## Objective

Build the evaluation tooling the quality and performance evaluations run:
(1) a workload driver that replays agent-turn traffic (recall/save mixes over
MCP and REST) against a seeded instance with configurable concurrency, and
(2) a recall-quality scorer that runs the golden query set from ADV-STORE-005
and reports ranking metrics (e.g. precision@k, expected-item rank).

## Outcome

After this advance:
- One command runs a defined workload profile and emits latency/throughput
  measurements with environment metadata captured alongside (required for
  performance evidence to be determinate, per the performance protocol).
- One command scores recall quality against the golden set and emits a
  comparable report, so ranking changes (ADV-CORE-002) are measurable, not
  vibes.
- Both are proven sensitive: a deliberately degraded configuration (negative
  control) visibly worsens the reported numbers, and a known-good baseline
  (positive control) reports as expected.

## Planned Work

- [ ] branch: create or confirm feature branch for this advance
- [ ] feat: workload driver (profiles, concurrency, latency capture, environment stamp)
- [ ] feat: recall-quality scorer over the golden query set
- [ ] prove sensitivity: negative + positive control runs documented
- [ ] operationalize: runnable locally and from CI

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: a harness that measures the wrong thing quietly blesses regressions —
  hence the mandatory control runs; oracle and ownership documented in-repo.
- Rollback: harness is tooling only; revert without production impact.

## Evidence

- [ ] profile:selected-practices
- [ ] enablement:artifact

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-GATEWAY-008 --status passed`

## Changes Made

- None yet
