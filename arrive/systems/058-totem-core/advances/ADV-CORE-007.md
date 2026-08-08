---
advance:
  id: "ADV-CORE-007"
  title: "Correct the workspace MSRV"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core"]
  started_at: "2026-08-07T20:00:00Z"
  implementation_completed_at: "2026-08-08T13:20:00Z"
  review_time_estimate_minutes: 10
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 7
  risk_flags: []
  evidence: ["msrv:verified"]
  practices:
    tidy_first:
      status: not_applicable
      rationale: "Single manifest field and one comment changed; nothing existing to prepare before the change."
    tdd:
      status: not_applicable
      rationale: "No production code behavior changed — this corrects a declared metadata value (rust-version) and a stale comment. The verification method is a build attempt on the declared toolchain, not a unit test; there is no red phase to write. No tdd:red-green is claimed."
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
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

- [x] branch / claim
- [x] determine the true minimum (e.g. `cargo msrv`, or the maximum
      `rust-version` across resolved dependencies)
- [x] update `Cargo.toml`; reconcile with the Dockerfile pin
- [x] verify a build on the declared version

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

- [x] msrv:verified — `cargo metadata --format-version 1` shows `fastnum
      0.7.5` declaring `rust_version = "1.94"`, the highest of any resolved
      dependency (`darling 0.23.0` and its macro crates are next at 1.88).
      Installed rust 1.94.0 with `rustup toolchain install 1.94.0 --profile
      minimal` and ran `rustup run 1.94.0 cargo build --workspace --locked`
      twice — once before the edit (to confirm the toolchain itself was
      sufficient) and once after (to confirm the edit didn't regress
      anything) — both finished clean with no errors.

## Changes Made

### 2026-08-08 - fix: correct declared workspace MSRV to 1.94

- `Cargo.toml`: `rust-version` changed from `1.85` to `1.94`, with a comment
  recording how the value was derived (max `rust_version` across
  `cargo metadata`) so the next dependency bump can re-check the same way
  instead of guessing.
- `Dockerfile`: updated the comment above the `rust:1.96-slim-trixie` pin,
  which cited the old (wrong) `1.85` MSRV as the reason for the gap between
  the pin and the declared version. The pin itself (`1.96`) is unchanged and
  is still ahead of the corrected `1.94`, so no inconsistency to reconcile
  beyond the stale comment.

## Check for Understanding

1. Why is `cargo metadata --format-version 1`'s per-package `rust_version`
   field a more reliable source than reading `Cargo.lock` or each
   dependency's `README` by hand?
2. `fastnum 0.7.5` needs `1.94`; `darling 0.23.0` needs `1.88`. If a future
   dependency bump lowers the effective requirement back under `1.94` (e.g.
   `fastnum` is downgraded), does this advance's fix silently become
   over-conservative, and how would a reviewer notice?
3. The Dockerfile pins `1.96`, one minor version ahead of the now-correct
   `1.94` MSRV. Is that gap intentional, and where is the reasoning for it
   recorded so it doesn't need re-deriving?
