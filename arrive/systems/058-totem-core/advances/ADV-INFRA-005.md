---
advance:
  id: "ADV-INFRA-005"
  title: "Verify the backup: restore a snapshot into a working gateway"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra"]
  started_at: "2026-08-07T20:00:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 20
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

ADV-INFRA-002 shipped with one requirement deliberately unmet: a snapshot was
created and Fly's scheduled snapshots are enabled, but **no restore has ever
been performed**. Its own evidence section says so, the runbook repeats it,
and the infra component's invariant is explicit — *backups must be
restorable*.

A backup that has never been restored is a hope. The estate now holds real
memories (the migrated demo records, and the first agent-written ones), so
the cost of the hope being wrong has stopped being hypothetical.

## Behavioral Change

After this advance:

- A snapshot is restored into a **new volume**, an app instance is brought up
  against it, and a recall returns the memories the snapshot contained —
  verified by reading them, not by the restore command exiting zero.
- The restore procedure in `infra/RUNBOOK.md` is corrected to what was
  actually done, including anything the documented steps got wrong, and the
  "not yet exercised end to end" caveat is removed **only because it is no
  longer true**.
- The measured restore time is recorded. An operator deciding whether to
  restore during an incident needs to know if it is two minutes or forty.
- DEP-001's single-owner rule is respected throughout: at no point do two
  machines hold the estate.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] snapshot; restore into a new volume; bring up against it
- [ ] verify by recall, not by exit code
- [ ] correct the runbook; record the timing
- [ ] clean up the temporary volume

## Scope and Boundaries

**In scope:** one executed restore, its verification, the runbook
correction, timing.

**Out of scope:** offsite backup (trial-grade data, by decision); automated
restore testing; point-in-time recovery.

## Risk + Rollback

- Risk: the restore rehearsal touches the live app's configuration. Do it
  against a *separate* volume and, if the app must be pointed at it, point it
  back afterwards and verify the original estate is intact — the rehearsal
  must not become the incident.
- Risk: discovering the backup does *not* restore. That is the point of
  doing it, and it would be a finding worth having before it matters.
- Rollback: destroy the temporary volume; nothing else changes.

## Evidence

- [ ] restore:executed — commands and output verbatim, plus the recalled
      memories proving the data came back
- [ ] timing recorded

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
