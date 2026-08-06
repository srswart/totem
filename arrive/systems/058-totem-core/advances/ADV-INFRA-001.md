---
advance:
  id: "ADV-INFRA-001"
  title: "Durable shared instance: RocksDB-backed gateway, backup/restore, client re-pointing"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra", "gateway", "store", "cli"]
  started_at: "2026-08-06T11:30:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["migration"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Implement DEP-001 ([docs/tech-direction/deployment.md](../../../../docs/tech-direction/deployment.md)):
turn the per-process, in-memory demo topology into the shared single instance —
`totem-gateway` as the one long-running process, owning an on-disk embedded
RocksDB store, with every other surface a client of the gateway. After this
advance, memories survive a restart and all surfaces see the same store: the
prerequisite for any harness capturing real memories in daily work.

WORKSTATION advance: developing and verifying durable on-disk state, restart
behaviour, and backup/restore needs a real machine and filesystem, and its
verification steps are interactive. Cloud routines skip it (see
docs/cloud-agent-notes.md, workstation gate).

## Behavioral Change

After this advance:

- `totem-gateway` started with a data directory (`TOTEM_DATA_DIR`) opens the
  store on RocksDB via the store's existing optional cargo feature; started
  without one, it keeps today's in-memory behaviour (explicitly labelled as
  ephemeral in its startup line). Restarting a durable gateway preserves
  memories, landscape, and access log.
- The single-owner invariant holds physically: a second process attempting to
  open the same data directory fails on the engine lock, with a clear error
  message pointing at the gateway.
- Backup and restore exist and are exercised: a documented, scriptable path
  (SurrealQL export or data-directory snapshot — chosen and justified in the
  implementation) produces a restorable artifact, and the advance's evidence
  includes an actual backup → wipe → restore → verify cycle.
- The CLI stops being a store island: `totem sync` and `totem credential`
  operate against the gateway (new gateway endpoints where needed) instead of
  constructing throwaway embedded stores. `totem enroll` already posts to the
  gateway.
- The MCP stdio binary's island status is resolved one of two ways — either it
  proxies to a running gateway, or it is explicitly documented as a
  self-contained dev tool pending ADV-GATEWAY-003's streamable-HTTP MCP (which
  serves from the gateway process itself). The choice is recorded in the
  advance, not left implicit.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: durable round-trip (write → drop store → reopen → read), engine
      lock contention error, backup/restore cycle — gated behind a cargo
      feature or marker so default cloud/CI builds stay in-memory and lean
- [ ] feat: gateway data-directory wiring, CLI re-pointing, backup/restore
      path, startup-mode labelling

## Bug Fixes

- [ ] None yet

## Scope and Boundaries

**In scope:** RocksDB wiring behind configuration, restart durability, engine
single-owner behaviour, backup/restore, CLI-to-gateway re-pointing, minimal
packaging note (how to run the durable gateway on a workstation).

**Out of scope:** auth on the gateway's endpoints (ADV-GATEWAY-003 —
security-critical, Opus-designated); multi-instance/server-mode deployment
(DEP-001 defers it; parity insurance already executed by ADV-STORE-006);
episodic retention/archival policy (gaps doc item 3); console SSO (§9).

## Risk + Rollback

- Risk (`migration` flag): first durable schema — from here on, migrations
  meet pre-existing data. The migration path from an empty/older data
  directory must be explicit, and the backup step must precede it.
- Risk: a backup that has never been restored is a hope, not a backup — the
  evidence must include the full restore cycle, per the infra component
  invariant.
- Risk: accidentally defaulting CI or cloud test builds onto RocksDB would
  slow every ephemeral runner; the durable engine stays behind an opt-in
  feature (component invariant).
- Rollback: remove the data-directory configuration and the gateway reverts
  to in-memory demo behaviour; the store crate's default features are
  untouched.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] backup-restore: executed cycle recorded verbatim

## CI Evidence Notes

- If CI jobs are enabled, link pipeline evidence (`ci:passed`) from PR/MR and
  default-branch runs. CI runs the in-memory suite only; the durable-engine
  tests are workstation evidence, recorded here like ADV-STORE-006's.
- Externally-run checks before merge: `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `arrive doctor artifacts`, `arrive plan check`, `arrive check --strict`,
  `arrive score`.

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation, grounded in the files actually
changed)
