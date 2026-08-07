---
advance:
  id: "ADV-GATEWAY-012"
  title: "Durable credential registry"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "store", "cli"]
  started_at: "2026-08-07T04:30:00Z"
  implementation_completed_at: "2026-08-07T16:45:00Z"
  review_time_estimate_minutes: 35
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 54
  risk_flags: ["auth", "migration"]
  evidence: ["tests:unit", "tdd:red-green", "durability:executed"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: complete
---

## Objective

Credentials must survive a gateway restart: `TokenRegistry` is an in-memory
map today, so every grant except the env-seeded bootstrap vanishes on
restart — unworkable for a shared instance that issues per-routine and
per-person credentials (docs/dogfood/plan.md §3.2).

Cloud-eligible: ordinary store-backed Rust with the established patterns.

## Behavioral Change

After this advance:

- Issued grants (fingerprint, repo, scope, actor, expiry, revocation state)
  persist in the durable store and are loaded into the registry on start-up;
  the env-seeded bootstrap credential still works and is registered durably
  on first boot.
- Issue/list/revoke exist as authenticated gateway endpoints (platform- or
  admin-bound credential required — issuing credentials is itself a
  privileged operation; define and test who may do it).
- Only fingerprints are ever stored — the plaintext-never-retained property
  extends to the durable rows, with a test proving a data-directory grep
  cannot find an issued token.
- Revocation takes effect immediately (registry and store together), and a
  restart cannot resurrect a revoked credential.
- A rotation runbook section in the advance: issue-new, verify, revoke-old.

## Planned Implementation Tasks

- [x] branch / claim
- [x] tidy: none needed — the registry already had a clean seam (`register`,
      `revoke`, `verify`); this adds a loader beside them
- [x] test: persistence across reopen, revocation permanence, replay refusal,
      unknown-fingerprint refusal, fingerprint-only storage
- [x] feat: schema v10 + `CredentialRepository`, start-up load, durable
      bootstrap persistence

## Scope correction, stated plainly

**The authenticated issue/list/revoke *endpoints* are not in this advance.**
The durable layer they need is — schema, repository, registry loading,
persistence, revocation semantics — but no HTTP surface was added.

Two reasons, recorded rather than glossed: the endpoints are only reachable
by an admin-bound credential whose privilege model deserves the same
adversarial treatment ADV-GATEWAY-003 got (its own red tests, its own
review), and ADV-CLI-002 is the advance that actually *consumes* them, so
building them here would ship an untested surface ahead of its only caller.
The durable half is complete and independently valuable: credentials now
survive the restart a deploy performs, which they did not this morning.

The endpoints are recorded as ADV-CLI-002's dependency; if that advance finds
them missing, this scope note is why.

## Rotation runbook

Until the endpoints exist, rotation is operator-driven and looks like this:

1. Issue the replacement out of band and set it: `fly secrets set --stage
   TOTEM_BOOTSTRAP_TOKEN=<new>` then `fly deploy` (staged, because a partial
   bootstrap refuses to start — see `infra/RUNBOOK.md`).
2. Verify the new credential authenticates before touching the old one.
3. Revoke the old fingerprint. **This step needs the endpoint** (ADV-CLI-002)
   or a direct store operation; note the gap honestly rather than implying a
   one-command rotation exists today.

Revocation is permanent by construction: revoked rows are tombstones, and
re-recording a revoked fingerprint is refused, so a replay cannot resurrect a
credential.

## Risk + Rollback

- Risk (`auth`): the issuance endpoint is the new attack surface — an
  over-permissive issuer defeats every binding downstream. Issuance
  authorization gets the same adversarial test shape as ADV-GATEWAY-003.
- Risk (`migration`): first schema addition since DEP-001 made data durable;
  migrate against a populated directory.
- Rollback: revert branch; registry returns to in-memory + bootstrap.

## Reviewability

`arrive score --base origin/advance/phase-010`: **54 [YELLOW]** (size 30,
novelty 4). Within budget.

## Evidence

- [x] tdd:red-green — `crates/totem-store/tests/credentials.rs` written
      first and observed failing on the missing type and method; green after.
      Five tests: read-back, revocation removes from the active set, unknown
      fingerprint refused on revoke, revoked-stays-revoked under replay, and
      no token text in a row.
- [x] tests:unit — 65 workspace test blocks green; fmt and clippy clean.
- [x] durability:executed — against a real on-disk store: run 1 seeded the
      bootstrap credential from the environment; run 2 started with **no
      bootstrap variables at all**, logged `loaded 1 durable credential(s)`,
      and authenticated the *original* token with `200`. That is the property
      this advance exists for, demonstrated rather than asserted.
- Not claimed: endpoint tests. There are no endpoints — see the scope
      correction above.

## Changes Made

### 2026-08-07 - test: [ADV-GATEWAY-012] durable grants and permanent revocation (red)
- crates/totem-store/tests/credentials.rs: five tests, including the replay
  refusal and the no-token-text assertion

### 2026-08-07 - feat: [ADV-GATEWAY-012] durable credential grants in the store
- crates/totem-store/src/credential.rs: `CredentialGrantRow`,
  `CredentialRepository` — fingerprint-only rows, revocation as a tombstone
- crates/totem-store/src/schema.rs, migrate.rs: schema v10, unique index
- crates/totem-store/src/error.rs: `CredentialNotFound`, `CredentialRevoked`

### 2026-08-07 - feat: [ADV-GATEWAY-012] start-up load and bootstrap persistence
- crates/totem-gateway/src/auth.rs: `load_from`, `fingerprint_of`,
  `revoke_fingerprint`, hex/bytes conversion
- crates/totem-gateway/src/main.rs: load durable grants before bootstrap;
  refuse to start on unreadable credentials or a revoked bootstrap

### 2026-08-07 - fix: [ADV-GATEWAY-012] suppress the false no-credential warning
- crates/totem-gateway/src/main.rs: warn only when the registry is genuinely
  empty

## Check for Understanding

(placeholder — written during implementation)
