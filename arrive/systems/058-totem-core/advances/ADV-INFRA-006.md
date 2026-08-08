---
advance:
  id: "ADV-INFRA-006"
  title: "Cache dependency builds so deploys are minutes, not quarter-hours"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra"]
  started_at: "2026-08-07T20:00:00Z"
  implementation_completed_at: "2026-08-08T08:55:00Z"
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 6
  risk_flags: []
  evidence: ["timing:measured", "cache-bust:demonstrated"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
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
- [x] measure cold and warm builds, before and after — see "Measured" below
- [x] prove a dependency change busts the cache — see "Measured", third column
- [ ] consider the console stage — `dx build` recompiles the console's wasm
      dependencies on every deploy for the same reason. Deliberately not
      folded in: `dx` drives its own cargo invocation with its own profile
      and flags, so whether a cooked cache is even reused there is a question
      to measure rather than assume. Measure its share of the build first;
      if it is small, say so and leave it.
- [x] update `infra/RUNBOOK.md` — replaced the known-cost note with what to expect per change type, and how to spot the cache silently failing

## Measured

Docker on the workstation (arm64), `--target builder`, faithful to the
deployed Dockerfile including `CARGO_BUILD_JOBS=2`. Absolute times differ
from Fly's amd64 remote builder; the **ratio and the layer split** are what
transfer.

| | cold | warm (source-only change) | dependency added |
|---|---|---|---|
| `cargo chef cook` — every dependency | **646.5s** | **CACHED** | **693.3s** |
| `cargo build` — this workspace only | 28.3s | 26.5s | 27.9s |
| embedder warm-up | 18.2s | 23.6s | 18.9s |
| **wall clock** | **11m 42s** | **52s** | **12m 21s** |

**13.5x, and the number that matters is the split.** 646s of 702s — **92% of
a cold build** — is dependency compilation, and it is now on a layer keyed to
`recipe.json` rather than to our source. `cargo build` at 28.3s cold against
26.5s warm confirms the cooked artifacts are genuinely being *reused* rather
than silently recompiled: if the `cook` and `build` flags had drifted, this
row would have jumped to several hundred seconds while `#12` still reported
`CACHED`. That is exactly the failure this measurement exists to catch, and
it did not happen.

**The cache busts, which is the more important of the two tests.** A cache
that serves stale dependencies is worse than no cache, so the third column
adds one real dependency (`hex = "0.4"`) to `totem-gateway`'s manifest. The
recipe changes, `#12` re-executes rather than reporting `CACHED`, and the log
shows it compiling `hex v0.4.3` — the new dependency is genuinely built, not
skipped. Cost: back to a full 12m 21s, exactly as it should be.

**Method notes.**

- The warm run changed file *contents*. `touch` alone would have proved
  nothing — Docker's `COPY` cache key is a checksum, not an mtime, so a
  touched file produces a cache hit that looks like success and measures
  nothing.
- `cook` compiles `totem-core` and the other workspace members even though
  that stage holds none of our source. This is `cargo-chef` writing dummy
  stub crates so dependency *feature resolution* matches the real build. It
  is compiling empty shells, and it is why `cook` is not purely third-party
  time.
- Measured with `--target builder`, so the console stage is excluded from
  every column. Its share of a real deploy is still unmeasured — see the
  open task.

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

- [x] timing: cold and warm, before and after, measured
- [x] cache-bust: a dependency change demonstrably rebuilds

## Changes Made

- `Dockerfile`: a shared `chef` base holding the toolchain and the apt
  packages (previously installed twice, once below `COPY . .` where a source
  change invalidated them), a `planner` stage producing `recipe.json`, and a
  `builder` stage that cooks dependencies from the recipe before copying any
  source.
- `infra/RUNBOOK.md`: the known-cost note replaced with expected build times
  per change type, and the symptom of the cache silently failing.

## On Fly: the first deploy, and a correction to the warm estimate

Deployed 2026-08-08, version 11, healthy. **34 minutes** — cold by
construction: the Dockerfile itself changed, so every layer in it was new to
Fly's builder and there was nothing to hit. This deploy bought no speed and
was never going to; it exists to populate the cache.

Per-stage, from the build output:

| stage | cold on Fly | next deploy |
|---|---|---|
| `chef` apt packages | 32.7s | CACHED |
| `cargo install cargo-chef` | 42.1s | CACHED |
| `cargo chef prepare` | 0.2s | re-runs, negligible |
| `cargo chef cook` | the bulk | CACHED unless a manifest changed |
| `cargo build` | — | re-runs, ~30-60s |
| `dx build` (console) | **90.1s** | **re-runs every time** |

**Two corrections to what this advance claimed.**

1. **34 minutes, against an estimate of 15-25.** Fly's amd64 builder is
   slower than the workstation for this work, and `CARGO_BUILD_JOBS=2` — kept
   deliberately, because the remote builder OOM-killed rustc without it
   (ADV-STORE-008) — costs parallelism that the local run also paid but
   evidently absorbed better. The local *ratio* still looks sound; the local
   *absolute times* under-predicted Fly by roughly 3x, which is worth
   remembering the next time a local measurement is used to set an
   expectation.

2. **The warm case will be ~3-5 minutes, not the ~1 minute the 52s figure
   suggested.** That figure was `--target builder`, which excludes the
   console stage entirely — recorded as a method note at the time, and now
   the reason the headline number was optimistic. `dx build` is 90.1s and
   `COPY . .` sits above it, so **any** source change invalidates the console
   stage, including a change touching only the gateway.

   The console stage's expensive layers (`cargo install dioxus-cli`, apt,
   rustup target) did come back `CACHED`, so that part works as designed.

**The cheap follow-up this measurement points at**, and the answer to the
console task above: chef-ing the console's dependencies is *not* the prize.
Narrowing its `COPY . .` to what the console actually needs is — it would let
a gateway-only change skip `dx build` altogether, which is the common case
for this project. Recorded here rather than done, because it is a Dockerfile
change wanting its own before/after.

## Deploy 2: the cache missed, and the reason is worth knowing

Deploy 2 (2026-08-08, version 12, ADV-GATEWAY-016) took **34m 41s** —
`cook` ran for **1986s** rather than reporting `CACHED`. The one-time layers
*did* cache, so Fly's builder retains layers between deploys:

```text
CACHED [chef 2/4] apt-get install ...
CACHED [chef 3/4] cargo install cargo-chef
CACHED [console 2..5/7] dioxus-cli, rustup target, apt
       [builder 2/5] cargo chef cook ...        1986.0s   <- NOT cached
```

**No dependency changed.** `git diff` across the two deployed commits shows
zero modifications to any `Cargo.toml` or to `Cargo.lock`.

Diffed the two `recipe.json` files directly, and the entire difference is:

```text
+[[test]]
+path = "tests/rank_explanation.rs"
+name = "rank_explanation"
+required-features = []
```

**Cargo auto-discovers `tests/*.rs` as targets, and `cargo-chef` bakes the
*resolved* manifest — auto-discovered targets included — into the recipe.**
Adding one integration test file therefore changed the recipe, missed the
cache, and cost a 33-minute dependency rebuild, without a single dependency
changing.

This generalises: **adding or removing any file cargo auto-discovers — a
test, a bench, an example, a bin — busts the whole dependency cache.**
Editing existing source does not, which is the common case and is why the
local warm measurement (52s) was not wrong, only narrow.

It is a property of the tool rather than a defect in this setup, and there is
no clean mitigation: declaring targets explicitly with `autotests = false`
moves the manifest change rather than removing it. So it is recorded to be
*expected* rather than solved — a deploy that adds a test file is a
12-minute deploy, and that is not a regression.

**Revised expectation, replacing the one in the runbook table:**

| what changed | expect |
|---|---|
| existing source only | the cook layer caches; ~3-5 min |
| a new test/bench/example/bin **file** | full rebuild, ~35 min |
| any `Cargo.toml` / `Cargo.lock` | full rebuild, ~35 min |

## Residual: the claim in the title is not yet proven on Fly

The title says *deploys* are minutes. Everything measured here is a
**workstation** build. Fly's remote builder is a different architecture, a
different machine, and — most importantly — a different cache lifetime: a
recycled builder starts cold no matter what this Dockerfile does.

The mechanism is proven and the ratio should carry, but "a source-only deploy
takes about a minute" is a claim about Fly's builder and remains unmade until
a deploy makes it. Record the first two real deploy times in this section
rather than assuming the local numbers transfer.

**Status after the first deploy (above): still open, and now with a specific
question.** Deploy 1 was cold by construction and tells us nothing about the
cache. **Deploy 2 is the test**, and the number to watch is whether
`cargo chef cook` reports `CACHED`. If it does not — if Fly's builder does
not retain layers between deploys — then this advance delivers nothing on Fly
whatever it delivers locally, and that finding matters more than the speedup
would have, because phase-014's calibration work assumes many
deploy-and-measure cycles.

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
