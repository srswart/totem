---
advance:
  id: "ADV-CLI-001"
  title: "totem CLI: enroll, sync hook install, actor credentials (gap-fill)"
  system: "058-totem-core"
  primary_component: "cli"
  components: ["cli", "gateway", "arrive-sync"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 40
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth"]
  evidence: []
  model_usage: []
  status: planned
---

## Objective

**Gap-fill** (see docs/arrive-decomposition-gaps.md): enrollment is a core flow
(Solution Intent §3.3) and the `cli` component exists in the decomposition, but
no roadmap advance builds it. Implement `totem enroll` (register repo, trigger
initial ARRIVE ingestion, install the sync hook) and actor enrollment
(obtain a scoped credential).

## Behavioral Change

After this advance:
- `totem enroll` in a repo registers it with the gateway, runs the first
  ingestion, and installs a git/CLI hook so subsequent `/arrive/` changes sync
  automatically — the coverage success measure ("% of advances visible within
  minutes of change") becomes reachable.
- A human or agent can obtain a least-privilege credential bound to repo +
  scope via the CLI.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: enroll flow tests against a test gateway (red first)
- [ ] feat: `totem enroll`, hook installer, credential commands

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: credential handling on developer machines (storage, leakage); hook
  must be safe to run in any repo state.
- Rollback: `totem unenroll` / hook removal; revoke issued credentials.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CLI-001 --status passed`

## Changes Made

- None yet
