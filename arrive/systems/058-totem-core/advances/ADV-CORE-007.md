---
advance:
  id: "ADV-CORE-007"
  title: "Correct the workspace MSRV"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core"]
  started_at: "2026-08-07T20:00:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 10
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

`Cargo.toml` declares `rust-version = "1.85"`. Nothing in the workspace can
build on it: `fastnum 0.7.5` requires 1.94 and `darling 0.23` requires 1.88.
It goes unnoticed because workstations and CI run newer toolchains — the
deployment is what exposed it, when a container pinned to the declared MSRV
refused to compile (ADV-INFRA-002).

A declared MSRV that no toolchain satisfies is a false statement in the
manifest. Small, but this project's whole practice is that records match
reality.

## Behavioral Change

After this advance:

- `rust-version` states a version the workspace can actually build on,
  determined by checking rather than guessing — the highest requirement in
  the dependency graph, verified by building with that toolchain.
- The Dockerfile's pin and the declared MSRV are consistent, or the
  difference is explained where a reader will find it.
- The advance records **how** the value was determined, so the next person to
  bump a dependency knows how to re-check rather than re-deriving it.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] determine the true minimum (e.g. `cargo msrv`, or the maximum
      `rust-version` across resolved dependencies)
- [ ] update `Cargo.toml`; reconcile with the Dockerfile pin
- [ ] verify a build on the declared version

## Scope and Boundaries

**In scope:** the declared MSRV and its consistency with the deployment pin.

**Out of scope:** an MSRV *policy* (how far back to support, whether to test
it in CI) — worth having, but a bigger decision than correcting a false
statement.

## Risk + Rollback

- Risk: declaring a version nobody verifies re-creates the same fiction one
  number higher. The advance requires an actual build on the declared
  version, not a reading of dependency manifests alone.
- Rollback: revert branch; the manifest returns to its current inaccuracy.

## Evidence

- [ ] msrv:verified — a build on the newly declared version, recorded

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
