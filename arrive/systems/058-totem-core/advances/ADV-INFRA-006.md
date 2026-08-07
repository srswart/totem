---
advance:
  id: "ADV-INFRA-006"
  title: "Cache dependency builds so deploys are minutes, not quarter-hours"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra"]
  started_at: "2026-08-07T20:00:00Z"
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
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Every deploy recompiles SurrealDB and RocksDB from scratch — 10 to 15 minutes
— because the Dockerfile's `COPY . .` invalidates Docker's cache on any
source change. Recorded as a known cost in ADV-INFRA-002 and its runbook.

It is not merely slow. It made a flaky network genuinely expensive (an
interrupted build restarted the whole compile, twice on 2026-08-07), it
discourages the small frequent deploys that a trial wants, and it will make
the double-deploy verification that DEP-001 depends on tedious enough to skip
— which is how a safety check quietly stops happening.

## Behavioral Change

After this advance:

- Dependency compilation is a cached layer: a source-only change rebuilds
  only this workspace's crates. The standard shape is `cargo-chef` (build a
  dependency recipe, cache it, then copy sources), but the advance may choose
  another approach and record why.
- The improvement is **measured**: cold build time and warm rebuild time,
  before and after. "Faster" without numbers is not a result.
- Cache correctness is proven, not assumed: a deploy after a dependency
  *version* change must rebuild dependencies, and the advance demonstrates
  that it does. A cache that serves stale dependencies is worse than no
  cache.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] restructure the Dockerfile for dependency caching
- [ ] measure cold and warm builds, before and after
- [ ] prove a dependency change busts the cache
- [ ] update `infra/RUNBOOK.md`, removing the known-cost note

## Scope and Boundaries

**In scope:** build caching in the deployment image, its measurement, and
cache-correctness evidence.

**Out of scope:** a remote build cache or CI build sharing; cross-deploy
artifact registries.

## Risk + Rollback

- Risk: a stale cache silently shipping old dependency code. This is the
  failure that makes caching dangerous rather than merely complex, hence the
  explicit bust test.
- Risk: added Dockerfile complexity for a trial-grade deployment. Justified
  by the double-deploy check DEP-001 depends on; if the numbers do not
  justify it, record that and revert.
- Rollback: restore the previous Dockerfile; deploys return to full rebuilds.

## Evidence

- [ ] timing: cold and warm, before and after, measured
- [ ] cache-bust: a dependency change demonstrably rebuilds

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
