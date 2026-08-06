---
advance:
  id: "ADV-INFRA-001"
  title: "Durable shared instance: RocksDB-backed gateway, backup/restore, client re-pointing"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra", "gateway", "store", "cli"]
  started_at: "2026-08-06T11:30:00Z"
  implementation_completed_at: "2026-08-06T13:05:00Z"
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 43
  risk_flags: ["migration", "concurrency"]
  evidence: ["tests:unit", "backup-restore:executed"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

Implement DEP-001 ([docs/tech-direction/deployment.md](../../../../docs/tech-direction/deployment.md)):
turn the per-process, in-memory demo topology into the shared single instance —
`totem-gateway` as the one long-running process, owning an on-disk embedded
RocksDB store, with every other surface a client of the gateway. After this
advance, memories survive a restart and all surfaces see the same store: the
prerequisite for any harness capturing real memories in daily work.

WORKSTATION advance: developed and verified on a workstation (the first
WORKSTATION advance executed through the phase machinery — claim-first on the
sub-branch, same discipline as cloud runs). Cloud routines skip it per the
workstation gate.

## Behavioral Change

After this advance:

- `totem-gateway` started with `TOTEM_DATA_DIR` (and built with the gateway's
  new `rocksdb` feature, which forwards to the store's existing one) opens the
  store on RocksDB at that directory via the new `Store::on_disk`. Restarting
  it preserves memories, landscape, and access log — executed, see Evidence.
- Started **without** `TOTEM_DATA_DIR`, it keeps the in-memory engine and
  says so loudly: `EPHEMERAL in-memory — memories are lost on exit`.
- Started with `TOTEM_DATA_DIR` but **without** the compiled feature, it
  refuses to start: an in-memory gateway that looks configured for durability
  would lose the team's memory silently — the worst failure shape (cf.
  TD-011's silent-discard lesson).
- The single-owner invariant holds physically: a second `Store::on_disk` on
  the same directory fails on the engine lock (tested), and the gateway's
  startup error explains the lock as DEP-001 doing its job.
- Backup and restore exist as `infra/backup.sh` / `infra/restore.sh`
  (offline directory snapshot of a closed data dir; restore moves the old
  directory aside rather than deleting it). The full
  backup → wipe → restore → verify cycle was executed for real — see
  Evidence.

**Corrections to this advance's original scope, stated plainly:**

- **CLI re-pointing was already done.** The advance was authored from the
  CLI-001 reports' mention of throwaway embedded stores, but the *merged*
  CLI implementation has no store islands: `totem enroll` posts to the
  gateway's `/enroll`, and `totem credential` is a local file store by
  design (its gateway-side verification is ADV-GATEWAY-003's). Nothing to
  re-point; the planned `totem sync` gateway endpoints were therefore not
  needed.
- **MCP island decision:** the `mcp_stdio` binary keeps its own embedded
  in-memory store and is hereby documented as a **self-contained dev tool**,
  not a deployment surface. The real agent-facing MCP endpoint will be
  ADV-GATEWAY-003's streamable-HTTP MCP served *from the gateway process*
  (hence sharing the durable store by construction). Proxying stdio to the
  gateway was considered and rejected as throwaway work against that
  imminent replacement.

## Planned Implementation Tasks

- [x] branch: `advance/sub/phase-007/ADV-INFRA-001` (claim-first)
- [x] tidy: none needed — the gateway main was already a single construction
      site; `connect_store()` was introduced as part of the feature change
- [x] test: durable round-trip, engine-lock single-owner refusal,
      snapshot backup/restore — `crates/totem-store/tests/durability.rs`,
      gated by `required-features = ["rocksdb"]`
- [x] feat: `Store::on_disk`, gateway `TOTEM_DATA_DIR` wiring + mode
      labelling + misconfiguration refusal, backup/restore scripts

## Bug Fixes

- [ ] None

## Scope and Boundaries

Unchanged from authoring, with the two scope corrections above. Auth stays
ADV-GATEWAY-003 (now formally dependent on this advance — wired into its plan
`dependencies` during this work, since notes don't gate but dependencies do);
server-mode deployment stays deferred per DEP-001; episodic retention stays
gaps-doc item 3.

## Risk + Rollback

- Risk (`migration` flag): first durable schema — from here on, migrations
  meet pre-existing data. `Store::migrate` is already idempotent and
  ledger-driven (verified against a reopened directory in the durability
  test); the backup step precedes any risky operation by documented
  procedure (`infra/backup.sh` first).
- Risk (`concurrency` flag, and a real finding): the SDK closes the embedded
  engine **asynchronously** — the RocksDB lock releases a moment *after* the
  last handle drops, because the router task runs `kvs.shutdown()` on
  channel close. A process exit (real restarts) never sees this; the
  durability tests document it with a bounded reopen retry. Anything that
  ever hot-swaps stores inside one process must account for it.
- Risk: a backup that has never been restored is a hope, not a backup — the
  executed cycle below is the evidence, and the restore script never deletes
  the directory it replaces.
- Rollback: unset `TOTEM_DATA_DIR` (or build without the feature) and the
  gateway reverts to the ephemeral demo behaviour; the store's default
  features are untouched, so no other crate or CI build changes.

## Evidence

- [x] tests:unit — `crates/totem-store/tests/durability.rs`: 3 tests
      (reopen survival incl. migration-ledger durability; engine-lock
      second-owner refusal; snapshot backup/restore), all green under
      `--features rocksdb`. **TDD disclosed honestly:** `Store::on_disk` was
      drafted before the tests; red was verified by stashing the
      implementation (both reopen tests fail compile on the missing
      constructor) and re-running green after restoring it. Not claimed as
      `tdd:red-green`.
- [x] backup-restore:executed — live cycle on 2026-08-06, workstation:
      durable gateway started (`durable (RocksDB at …/totem-data)` startup
      line) → memory saved via REST → **process killed and restarted** →
      memory recalled intact → gateway stopped → `infra/backup.sh` snapshot
      → data directory deleted → `infra/restore.sh` → gateway restarted →
      memory recalled intact again.
- Not claimed: `ci:passed` for the durable path — CI builds default features
      only and never runs the durability tests, per the infra component's
      lean-default invariant. The workspace suite (default features) is green
      alongside; fmt/clippy clean in both feature states.

## CI Evidence Notes

- CI intentionally cannot produce the durable-path evidence (feature is
  opt-in); it is workstation evidence recorded here, like ADV-STORE-006's.

## Changes Made

### 2026-08-06 - docs: [ADV-INFRA-001] make GATEWAY-003 depend on INFRA-001
- arrive/implementation-plan.yaml: GATEWAY-003 `dependencies` gains
  ADV-INFRA-001 — the ordering lived only in a note, and notes don't gate

### 2026-08-06 - test: [ADV-INFRA-001] durability, engine-lock, snapshot backup/restore
- crates/totem-store/tests/durability.rs: new — reopen survival, physical
  single-owner, backup/restore; documents the async engine-close semantics
- crates/totem-store/Cargo.toml: `[[test]]` gate (`required-features`),
  tempfile dev-dependency

### 2026-08-06 - feat: [ADV-INFRA-001] Store::on_disk (embedded RocksDB, DEP-001)
- crates/totem-store/src/store.rs: `Store::on_disk` behind the existing
  `rocksdb` feature; documents the exclusive lock as the physical
  single-owner mechanism

### 2026-08-06 - feat: [ADV-INFRA-001] durable gateway mode + snapshot backup/restore
- crates/totem-gateway/src/main.rs: `connect_store()` — durable with
  `TOTEM_DATA_DIR`, loudly ephemeral without, hard refusal on
  durable-looking misconfiguration; module doc rewritten for DEP-001
- crates/totem-gateway/Cargo.toml: `rocksdb` feature forwarding to the store
- infra/backup.sh, infra/restore.sh: offline snapshot pair; restore moves
  the old directory aside, never deletes

## Check for Understanding

1. Starting the gateway with `TOTEM_DATA_DIR` set but the `rocksdb` feature
   not compiled in exits with an error instead of falling back to in-memory.
   What failure shape is that refusal preventing, and which executed TD
   finding is the precedent for treating "silently pretends to work" as
   worse than "refuses to start"?
2. The durability tests' `open()` helper retries for up to five seconds,
   but `the_engine_lock_refuses_a_second_owner` deliberately calls
   `Store::on_disk` without the retry. Why would adding the retry there
   destroy the test's meaning, and what SDK behaviour makes the retry
   necessary everywhere else?
3. `infra/restore.sh` moves an existing data directory aside instead of
   deleting it. Which infra component invariant does that serve, and what
   operator mistake does it survive that `rm -rf && cp` would not?
4. The advance corrected two pieces of its own authored scope (CLI
   re-pointing, `totem sync` endpoints). What did the authoring rely on
   that turned out to describe the *losing* CLI-001 implementation, and
   where does the winning implementation put each concern instead?
5. Why does the phase plan now express "auth lands on a durable gateway"
   as a `dependencies` edge on ADV-GATEWAY-003 rather than as the note it
   originally was — concretely, which protocol rule reads dependencies,
   and what could an Opus run have done under the note-only version?
