---
advance:
  id: "ADV-INFRA-003"
  title: "Dogfood cutover: enroll, credentials, harness wiring, memory discipline, measurement"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra", "gateway", "cli"]
  started_at: "2026-08-07T04:30:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 40
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

The switch-flip (docs/dogfood/plan.md §3.3, §4): every session that builds
Totem attaches to the hosted instance and uses it. After this advance the
dogfood trial is running, and its measurement is defined.

WORKSTATION advance: routine configuration, credential issuance, and
CLAUDE.local.md are human-held.

## Behavioral Change

After this advance:

- The repo is enrolled on the hosted instance; the post-commit sync hook
  points at it; the landscape stays live from workstation merges.
- Credentials exist per identity — `cloud-opus`, `cloud-sonnet`, `shawn` —
  project-scoped, issued through ADV-GATEWAY-012's registry; the rotation
  runbook has been exercised once.
- Both cloud routines carry the Totem MCP connector with their credential;
  the workstation session has the gateway registered via
  `claude mcp add --transport http`.
- docs/cloud-agent-notes.md gains the memory discipline: recall before
  reading governance docs (query the advance id + touched components); save
  decisions, dead ends, and spec corrections — not diff dumps; `totem_advance_log`
  on completion; `totem_feedback` when a recalled memory helped or misled.
  CLAUDE.local.md gets the workstation equivalent.

  **Scope narrowed 2026-08-07:** authoring the discipline moved to
  ADV-INFRA-004, which puts it in `arrive/agent-rules/` so
  `arrive sync agent-rules` reaches Claude Code, Cursor and the skills
  surface from one source. Putting it in `CLAUDE.local.md` would have made
  the rules for a *shared* memory system local to one machine. This advance
  keeps the credentials, connector wiring, and measurement.
- The measurement is defined and collection starts: per-run recall/save
  counts and feedback ratio (from Totem's own access log — it is the
  metering system), plus one journey-report line per run: "did a recalled
  memory change what you did? which?" — the honest-self-report instrument
  the overnight experiment's twin-project question needs.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] enroll + credentials + connector/harness wiring
- [ ] protocol + CLAUDE.local.md memory discipline
- [ ] first instrumented runs observed end-to-end (one cloud, one
      workstation), access-log evidence recorded

## Risk + Rollback

- Risk (`auth`): connector configs hold bearer credentials — per-identity
  tokens mean one leak revokes one identity, and rotation is a runbook
  step, not an incident.
- Risk: memory discipline that is too heavy gets ignored, too light saves
  noise; start minimal (the four rules above) and let the observation log
  drive tuning — process learnings are exactly what this trial is for.
- Rollback: detach connectors, revert protocol sections; the file-based
  memory and journey reports never stopped working.

## Evidence

- [ ] cutover:executed (one cloud run and one workstation session recalling
      and saving against the hosted instance, access-log excerpts verbatim)

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
