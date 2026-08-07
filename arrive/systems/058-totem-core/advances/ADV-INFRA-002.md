---
advance:
  id: "ADV-INFRA-002"
  title: "Hosted deployment: container image, TLS, supervision, daily backup"
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
  risk_flags: ["new_dependency"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Put the durable gateway on a small always-on cloud host (decision recorded in
docs/dogfood/plan.md) with trial-grade operations: TLS on a public HTTPS
endpoint, restart supervision, and a daily local backup. Deliberately simple
per the decision — no orchestration, no offsite backup requirement (data is
trial-grade), no allowlists beyond bearer auth + TLS.

WORKSTATION advance: provisioning a host, DNS, and TLS are human-held
operations.

## Behavioral Change

After this advance:

- A container image builds the gateway with `rocksdb` (and, once
  ADV-STORE-008 lands, `fastembed`) features; `infra/` gains the compose file
  (gateway + Caddy TLS termination) and a short runbook (provision, deploy,
  upgrade, restore).
- The gateway serves `https://<host>/mcp` and the REST surface publicly,
  bearer-authenticated, fail-closed; plain HTTP redirects or refuses.
- Supervision: the gateway restarts on failure (compose restart policy is
  sufficient); the DEP-001 single-owner lock error remains loud in logs.
- A daily cron runs `infra/backup.sh` against a stopped-or-consistent
  strategy documented honestly (stop-copy-start window, or accept the
  documented risk of a hot copy for the trial — choose and record).
- The workstation `~/.totem/data` estate can be migrated to the host with
  the restore script (do it as part of verification, so the trial starts
  with the memories already accumulated).

## Planned Implementation Tasks

- [ ] branch / claim (its phase will be active)
- [ ] image + compose + Caddy config under infra/
- [ ] provision host, deploy, verify /mcp handshake + 401 path over TLS
- [ ] supervision + daily backup cron; runbook
- [ ] migrate the workstation data estate; verify recall

## Risk + Rollback

- Risk (`new_dependency`): Caddy and a container runtime join the
  operational surface — both pinned in the compose file.
- Risk: the memory estate now lives on a rented host; trial-grade by
  decision, but the runbook's restore path is the safety net — verify it
  once on the host.
- Rollback: DNS off, container down; the workstation deployment mode
  (DEP-001) still works unchanged.

## Evidence

- [ ] deployment:executed (handshake + auth refusal over public TLS,
      verbatim)
- [ ] backup-restore: executed once on the host

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
