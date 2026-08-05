---
advance:
  id: "ADV-GATEWAY-007"
  title: "Performance evaluation: time-to-context + recall latency under load"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "store"]
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
  mode: evaluation
  facets: [performance]
  work_products: []
  status: planned
---

## Objective

Evaluate the "time-to-context" success measure and recall/save latency under
realistic concurrent load, using the workload driver (ADV-GATEWAY-008) on the
seeded corpus (ADV-STORE-005). The one-round-trip promise (G2) only matters
if that round trip is fast when many agents hit it at once.

## Outcome

After this advance:
- Measured p50/p95 latency for recall, save, and landscape under defined
  workload profiles, with thresholds passed/failed explicitly and findings
  (hot spots, scaling limits) dispositioned.
- A baseline recorded so later changes (embedding placement, ranking cost,
  curator load) can be compared instead of guessed.

## Planned Work

- [ ] branch: create or confirm feature branch for this advance
- [ ] define workload profiles + thresholds with team sign-off
- [ ] baseline runs; concurrency ramp; curator-running-concurrently run
- [ ] disposition findings; record baseline for future comparison

## Bug Fixes

- [ ] None yet

## Performance Question or Hypothesis

- Recall (scope-chain merge + vector + graph + ranking) stays within an
  agent-acceptable budget (target set at evaluation time, e.g. p95 under a
  few hundred ms) at team-scale concurrency, and does not degrade
  disproportionately while a curator job runs.

## Workload Model

- Profiles derived from the read→think→write turn shape: recall-heavy agent
  sessions, save bursts at turn end, landscape queries at session start;
  corpus sized to a plausible team-year of memories (documented in the run).

## Environment

- Dedicated instance with pinned SurrealDB version and hardware documented
  per run — missing environment provenance makes results indeterminate, so
  the harness stamps it automatically (ADV-GATEWAY-008).

## Measures and Thresholds

- p50/p95/p99 latency per operation, throughput at target concurrency, error
  rate; thresholds fixed before runs, not after.

## Baseline and Comparison

- First run establishes the baseline; subsequent evaluations compare against
  it (ranking on/off, embedding placement variants, curator on/off).

## Findings and Limitations

- To be completed at evaluation time; synthetic corpus and single-node limits
  noted as known caveats.

## Risk + Rollback

- Risk: load runs only against dedicated test instances.
- Rollback: n/a — findings only.

## Evidence

- [ ] profile:selected-practices
- [ ] profile:honest-practices
- [ ] analysis:artifact

## CI Evidence Notes

- Attach workload-driver reports (with environment stamps) as the analysis
  artifact.

## Changes Made

- None yet
