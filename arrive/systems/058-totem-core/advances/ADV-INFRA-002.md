---
advance:
  id: "ADV-INFRA-002"
  title: "Fly.io deployment: image, single-machine volume, secrets, snapshots"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra", "gateway"]
  started_at: "2026-08-07T04:30:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 40
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["new_dependency", "concurrency"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Put the durable gateway on **Fly.io** (app `totem-dev`, region **`sin`**,
created 2026-08-07) so it has a stable public HTTPS endpoint —
`https://totem-dev.fly.dev` — that cloud agents, the WorkOS Resource
Indicator, and the console can all reach. Trial-grade operations by decision:
no orchestration, no offsite backup mandate, no allowlist beyond OAuth/bearer
auth and TLS.

Fly changes the shape of this advance versus the generic-VPS version it
replaces: **TLS, supervision, and restart-on-failure come from the platform**
(no Caddy, no compose, no cron supervisor), while **machine count, volumes,
and secrets become the real work** — because DEP-001's single-owner rule
collides with every default Fly does.

WORKSTATION advance: provisioning, secrets, and volume operations are
human-held.

## The dominant constraint: exactly one machine, always

DEP-001 makes the gateway the *sole* owner of an embedded RocksDB store, and
the engine enforces it with an exclusive lock. Fly's defaults fight this:

- **Default deploys create two machines** for availability. A second machine
  cannot attach the same volume, and even if it could, the lock would refuse
  it. The app must be pinned to **exactly one machine**.
- **Rolling deploys run old and new simultaneously.** The new machine would
  find the volume locked (or unattachable) and fail. Deploys must stop the
  old machine before starting the new one (`immediate` strategy, or an
  explicit stop/deploy/start), and the advance must record which was used and
  the downtime it implies (seconds; acceptable for a trial).
- **Auto-stop/auto-start** saves money on an hourly-traffic service but adds
  wake latency and a shutdown path RocksDB must survive. Recommendation for
  the trial: **disable auto-stop**, prefer data integrity over pennies, and
  record the cost.

Every one of these is a configuration line that is silently wrong by default;
the advance's job is to make them explicit and *test* the result (deploy
twice in a row and confirm the second deploy comes up clean).

## Behavioral Change

After this advance:

- A **Dockerfile** builds the gateway with the `rocksdb` feature (and
  `fastembed` once ADV-STORE-008 lands); a committed **`fly.toml`** pins:
  app `totem-dev`, `primary_region = "sin"`, exactly one machine, the
  deploy strategy above, the mounted volume, and a VM size chosen with the
  embedder in mind (the default 256MB shared-cpu-1x is too small for
  SurrealDB + RocksDB, and BGE-small needs materially more — size it, then
  record measured memory rather than guessing).
- A **Fly volume** mounted at the gateway's `TOTEM_DATA_DIR`, in `sin`,
  holding the memory estate. Volume snapshots are the primary backup;
  `infra/backup.sh` remains usable via `fly ssh console` and its
  stop-copy-start honesty note still applies.
- **Secrets via `fly secrets set`**, never in `fly.toml`: the bootstrap
  credential and, once ADV-GATEWAY-013 lands, the OAuth issuer/JWKS
  configuration. `fly.toml` holds only non-secret configuration.
- **`TOTEM_MCP_ALLOWED_HOSTS=totem-dev.fly.dev`** — MCP-012 means the
  deployment refuses its own public hostname without this.
- **An unauthenticated health endpoint.** Fly health checks need a route that
  answers without credentials, and today every route is authenticated
  (the same class of problem MCP-014 found for OAuth discovery). Add a
  minimal `GET /health` outside the auth layer that reveals nothing but
  liveness — narrow, tested, and enumerated alongside the `.well-known`
  exception rather than quietly widening the auth boundary.
- **The workstation estate is migrated**: `~/.totem/data` is uploaded to the
  volume so the trial starts with the memories already accumulated, and a
  recall against the deployed instance proves it arrived.
- **A short runbook** in `infra/`: deploy, upgrade, roll back, restore from
  snapshot, and the "why exactly one machine" warning where the next person
  will actually read it.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] Dockerfile (rocksdb feature; slim runtime image) + `fly.toml`
      (single machine, `sin`, volume mount, deploy strategy, VM size)
- [ ] `GET /health` outside the auth layer, with a test proving nothing else
      escaped the auth boundary with it
- [ ] volume create + secrets set + first deploy; verify `/mcp` handshake and
      the 401 path against `https://totem-dev.fly.dev`
- [ ] deploy a second time and confirm a clean single-machine rollover
      (this is the test that the lock constraint is actually configured)
- [ ] migrate `~/.totem/data`; verify recall against the deployed instance
- [ ] snapshot/restore verified once on the real volume; runbook written

## Scope and Boundaries

**In scope:** image, `fly.toml`, volume, secrets, health endpoint, first
deploys, estate migration, snapshot/restore verification, runbook.

**Out of scope:** the OAuth resource server itself (ADV-GATEWAY-013 — this
advance only carries its configuration); the real embedder (ADV-STORE-008,
which will change the VM size and should re-measure); a custom domain (the
`.fly.dev` hostname is sufficient for the trial, and the WorkOS Resource
Indicator is registered against it); multi-region or scale-out, which
DEP-001 forbids by design.

## Risk + Rollback

- Risk (`concurrency`, the big one): every Fly default — two machines,
  rolling deploys, auto-start — violates DEP-001's single-owner rule, and
  the failure mode is a machine that cannot open the store rather than
  silent corruption (the lock is doing its job). Mitigated by explicit
  configuration *and* by the double-deploy test, which is the only thing
  that proves the configuration rather than asserting it.
- Risk: the volume is the memory estate. Fly volumes are single-host storage,
  not replicated; snapshots are the safety net and must be *verified by
  restoring one*, not assumed. Trial-grade data by decision, but a verified
  restore is cheap.
- Risk (`new_dependency`): Fly becomes the deployment substrate; the runbook
  keeps `fly` command knowledge out of one person's head, and DEP-001's
  workstation mode still runs unchanged if Fly is ever abandoned.
- Risk: a public endpoint appears before ADV-GATEWAY-013's OAuth lands. It is
  bearer-authenticated and fail-closed from the first deploy (the hardening
  pair GATEWAY-009/CORE-006 are already merged), so exposure is acceptable —
  but do not disable auth "temporarily" to debug a deploy.
- Rollback: `fly apps destroy` (or scale to zero) and the workstation
  deployment mode is untouched — the estate exists in both places during the
  trial's early days.

## Evidence

- [ ] deployment:executed — `/mcp` handshake and 401 refusal against
      `https://totem-dev.fly.dev`, verbatim
- [ ] single-machine:verified — two consecutive deploys, second comes up
      clean; `fly status` showing exactly one machine
- [ ] backup-restore: a volume snapshot restored and read back, executed

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
