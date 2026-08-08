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

## Approach: cargo-chef, and the one way it fails quietly

Chosen: **`cargo-chef`**, the shape the advance already named. The workspace
is reduced to a `recipe.json` — a manifest-only description of the dependency
graph, containing none of our source — which a `cook` step compiles on its
own layer. Docker then caches that layer against the recipe, so it is reused
until a *dependency* changes and a source-only change skips SurrealDB,
RocksDB and ONNX Runtime entirely.

Rejected: a mounted cargo registry/target cache (`--mount=type=cache`). It is
fewer lines and it does not survive Fly's remote builder being recycled,
which is precisely the situation that made the 2026-08-07 rebuilds expensive.
A cache that disappears when the machine does is not the cache this advance
is asking for.

**The failure mode worth naming.** `cargo chef cook` compiles with whatever
flags it is given. If they do not match the real `cargo build` — a different
feature set, a different package, a different profile — Docker still reuses
the cooked layer, and then `cargo build` rebuilds every dependency anyway
because the fingerprints differ. **The build costs full price while looking
like a cache hit**, and nothing in the output says so. The two invocations
are therefore kept adjacent in the Dockerfile with a comment, and the warm
measurement below is what would actually catch it: a warm build that is not
dramatically faster means the flags have drifted.

Also folded in: the builder previously ran `apt-get install` twice, once
before `COPY . .` and once after, the second adding `build-essential`. Both
now sit in a shared base stage above any source copy, where a source change
cannot invalidate them.

## Planned Implementation Tasks

- [x] branch / claim
- [x] restructure the Dockerfile for dependency caching
- [ ] measure cold and warm builds, before and after
- [ ] prove a dependency change busts the cache
- [ ] consider the console stage — `dx build` recompiles the console's wasm
      dependencies on every deploy for the same reason. Deliberately not
      folded in: `dx` drives its own cargo invocation with its own profile
      and flags, so whether a cooked cache is even reused there is a question
      to measure rather than assume. Measure its share of the build first;
      if it is small, say so and leave it.
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

1. `cargo chef prepare` still runs after `COPY . .`, so that stage rebuilds on
   every source change. Why does that not defeat the caching, and what
   property of its *output* is doing the work?
2. `cook` and `cargo build` must be given identical flags. Describe what
   happens if they are not — and say why it is worse than having no cache at
   all rather than merely no better.
3. Which measurement would reveal that mismatch, and which would not?
4. A mounted cargo cache is fewer lines than a chef stage. Name the specific
   situation from 2026-08-07 that argues against it.
5. The apt-get installs moved above `COPY . .`. What was being invalidated
   before, and roughly what did it cost per deploy?
6. The advance requires proving a dependency change *busts* the cache, not
   just that a source change hits it. Why is that the more important of the
   two tests?
