---
advance:
  id: "ADV-INFRA-008"
  title: "Build parallelism is computed, and the one network step is retried"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra"]
  started_at: "2026-08-08T11:30:00Z"
  implementation_completed_at: "2026-08-08T12:05:00Z"
  review_time_estimate_minutes: 15
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 4
  risk_flags: ["deployment"]
  evidence: ["timing:measured", "build:executed", "tests:manual"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

ADV-INFRA-006 cached dependency compilation and left the *cold* path at
34 minutes on Fly. Investigating why turned up a one-line cause:
**`CARGO_BUILD_JOBS=2`**, set during ADV-STORE-008 to stop an OOM, and never
revisited. It throttles the step that is ~94% of a cold build to two
concurrent compiles.

Measured locally — the same Dockerfile, the same machine, only the job count
changing:

| jobs | `cargo chef cook` | `cargo build` (ours) |
|---|---|---|
| 2 | 646.5s | 28.3s |
| 6 | 261.7s | 14.1s |
| 7 | 303.5s | — |
| 10 | 206.9s | 13.4s |

**Read this table for its direction, not its precision.** Each row is a
single run on a workstation that was not otherwise idle, and the 7-job run
landing *slower* than the 6-job one proves the run-to-run noise is larger
than the gaps between 6, 7 and 10. Averaged runs on a quiet machine would be
needed to locate the knee, and that is not worth the hours it would cost.

What the table does support:

- **Going from 2 to "as many as the builder can feed" is worth roughly 2-3x**
  on the dependency compile. That gap is far outside the noise.
- **The knee is somewhere around 6-10 and is not resolvable from this data.**
  Any specific claim about where would be over-reading it.
- **The warm path gains too**: our own crates roughly halve, so a source-only
  deploy is faster even when the dependency cache hits.

The flattening after 6 is expected on theory — the critical path
(`surrealdb-core`, `ort-sys` are largely serial internally) sets a floor no
job count goes below — but this data is too noisy to claim it was *observed*.

## Two decisions, and why neither is a magic number

### The job count is computed, not chosen

```sh
min(cores, RAM_in_GB), floored at 2
```

**The constraint was never cores — it is memory per concurrent `rustc`.**
That is why the original OOM happened: cargo defaults to core count, on a
builder whose RAM could not feed that many linkers. A literal encodes an
assumption about a machine we do not control, and gets it wrong in one
direction or the other: too low wastes most of the available speed, too high
fails the deploy.

Fly's builder is a **managed Depot** builder — there is no `fly-builder-*`
app to inspect — so its shape is not visible from here and can change without
notice. Computing the value moves the decision to the machine that has the
answer.

One gigabyte per job is a rule of thumb, but a measured one: ten concurrent
jobs completed this dependency graph on 7.7 GB, so ~0.8 GB/job sufficed and
1.0 leaves headroom. `--build-arg CARGO_BUILD_JOBS=2` overrides it if a build
ever misbehaves.

### The `jobs=10` run failed, and not for the reason it appears

It died two steps after both compiles succeeded:

```text
Failed to retrieve onnx/model.onnx
```

At 0.181s — instant, so not a timeout. A rate limit, after four full builds
in two hours. **Nothing to do with the job count**, and it would have been
easy to record a tidy "10 jobs is too many for 8 GB" conclusion that was
entirely wrong. The same trap as `surrealdb-core` naming the OOM victim
rather than the cause: *the failing step is not the causing step*.

That accident exposed a real fragility. **The warm-embedder step is the
build's only reach to the public internet**, and it had no retry: a transient
upstream problem fails a deploy at a step unrelated to anything that changed.
Now retried three times with a widening pause.

This is a mitigation, not a fix. A real outage still fails the build, and the
durable answer is to vendor the weights into a base image — which would also
remove the ~276s cold download entirely. Recorded as the next step rather
than done here, because it is a separate change with its own tradeoff (a
second image to build and keep current).

## Behavioral Change

- A cold build uses as much parallelism as its builder can feed, instead of 2.
- The same Dockerfile is correct on an 8 GB workstation VM and on whatever
  Fly provides, without either being special-cased.
- A transient model-download failure retries instead of failing the deploy.
- The chosen job count is **printed** in the build log, so a surprising build
  time can be diagnosed from the output rather than guessed at.

## Scope and Boundaries

**In scope:** the computed job count, the retry, and their measurement.

**Out of scope:** vendoring the model weights (recorded above); the console
stage's `COPY . .`, which ADV-INFRA-006 identified as the remaining warm-path
cost; a remote build cache.

## Risk + Rollback

- Risk (`deployment`): more concurrent compiles is more peak memory, and an
  OOM on Fly fails the deploy. Bounded by the RAM term, and 6 jobs is
  validated end-to-end locally while 2 remains one `--build-arg` away. Note
  this is the one risk the local measurements cannot bound: Fly's builder RAM
  is unknown, and the formula's whole purpose is to adapt to it.
- **Known limitation:** the layer that computes the count is itself cached by
  Docker, and `nproc`/`MemTotal` are not part of its cache key. If the
  builder changes shape, a stale count can persist until something busts that
  layer. The direction of the risk depends on which way the builder moved,
  and the override is the escape hatch. Recorded rather than solved: keying
  the layer on the machine's resources would bust the dependency cache on
  every builder change, which costs more than it saves.
- Rollback: revert; builds return to 2 jobs.

## Evidence

- [x] timing:measured — job counts compared on one machine, one variable.
      Direction only; see the note under the table on why the knee is not
      resolvable from this data.
- [x] build:executed — the full builder stage with the computed count:
      `cargo build jobs: 7 (nproc=12, 7.7GB)`, exit 0 in 5m42s.
- [x] tests:manual — **the retry's failure branch did not execute in that
      build**: the download succeeded first time, because the rate limit that
      prompted this had cleared. Shipping unexercised error-handling would be
      the same mistake this advance is about, so the loop was verified in
      isolation against a command that fails twice then succeeds (recovers,
      exit 0) and one that never succeeds (three attempts, `FATAL`, exit 1).
- [x] deployment:executed — 2026-08-08, cold build on Fly with the computed
      count: `cargo chef cook` **715.0s against 1986.0s** at `jobs=2`, a
      **2.78x** improvement, and `cargo build` 31.8s against 72.2s. Whole
      build ~13 minutes against the 34-minute baseline.

      The local measurement over-predicted the ratio (2-3x locally, 2.78x
      here) but got the *direction and rough size* right — which is what
      ADV-INFRA-006 said would transfer and what its absolute seconds did not.

## Check for Understanding

1. The job count is computed from `min(cores, RAM_in_GB)` rather than pinned.
   Name the failure each half of that expression prevents.
2. `CARGO_BUILD_JOBS=2` was set to fix a real OOM and was correct when
   written. What made it wrong later, and what does that suggest about
   constants introduced to stop a specific failure?
3. The `jobs=10` build failed. Say why that is *not* evidence about the job
   count, and name the earlier defect in this project with the same shape.
4. The retry is described as a mitigation rather than a fix. What does it not
   survive, and what would?
5. Going from 6 to 10 jobs bought only 1.27x. Give two independent reasons
   the curve flattens.
6. The computed-jobs layer is cached and its inputs are not in the cache key.
   Describe the stale-value scenario, and say why keying the layer on
   `nproc` would be a worse trade.
