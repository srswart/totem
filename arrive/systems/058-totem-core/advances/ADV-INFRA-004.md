---
advance:
  id: "ADV-INFRA-004"
  title: "Memory discipline as synced agent rules and a Totem skill"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra"]
  started_at: "2026-08-07T19:00:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
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

Give every harness that touches this repo — Claude Code (workstation and
cloud), Cursor, and any future enrolled developer's — the rules and the
on-demand detail for *using* Totem, through the repo's own governance
pipeline rather than one person's untracked local file.

ADV-INFRA-003 currently plans to put the memory discipline in
`docs/cloud-agent-notes.md` (cloud routines only) and `CLAUDE.local.md`
(explicitly local, never synced, never committed). That reaches neither
Cursor nor a second developer — an odd shape for a system whose premise is
*shared* memory, and one that guarantees the discipline drifts per machine.

The repo already has the right mechanism and nothing plans to use it:
`arrive/agent-rules/*.md` plus `arrive sync agent-rules` generates
`CLAUDE.md`, `.cursor/rules/*.mdc`, and `.claude/skills/*` from one canonical
source.

## Behavioral Change

After this advance:

- **A canonical rule** at `arrive/agent-rules/memory-discipline.md` (with its
  `.frontmatter.yaml`) is the single source for how sessions use Totem. It is
  always-applied rather than glob-scoped: it governs what a session does at
  its start and end, not what it does when editing a particular file.
- **`arrive sync agent-rules` propagates it** to `CLAUDE.md`, a Cursor rule,
  and a `totem-memory` skill — so Claude Code and Cursor receive the same
  discipline, and it arrives with a clone rather than with a person.
- **The rule stays short**; the skill carries the detail. The rule says what
  to do (recall before reading governance docs; save decisions, dead ends and
  spec corrections — not diff dumps; `totem_feedback` when a recalled memory
  helped or misled). The skill explains the scope model, what belongs in each
  memory category, and what *not* to save.
- **Connection instructions are documented once**: `claude mcp add --transport
  http` with a bearer credential for workstation Claude Code, the connector
  for claude.ai routines, and whatever Cursor's equivalent proves to be —
  including, honestly, "unverified" if it is (mcp.md's Cursor rows are still
  unverified, and this advance does not get to pretend otherwise).
- **ADV-INFRA-003's scope narrows** accordingly: it wires credentials,
  connectors and measurement, and no longer authors discipline into
  `CLAUDE.local.md`.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] author `arrive/agent-rules/memory-discipline.md` + frontmatter
- [ ] `arrive sync agent-rules`; verify the generated Claude, Cursor and
      skill outputs are what the rule intended
- [ ] document connection for each harness; mark unverified ones as such
- [ ] narrow ADV-INFRA-003's scope note

## Scope and Boundaries

**In scope:** the canonical rule, its generated outputs, the skill's content,
connection documentation, and the INFRA-003 scope narrowing.

**Out of scope:** the credentials themselves and the measurement
(ADV-INFRA-003); CLI authentication (ADV-CLI-002); whether the discipline is
*any good*, which only running it will tell — this advance ships v1 and the
observation log tunes it.

## A component-ownership gap this exposes

`arrive/agent-rules/**`, `.cursor/**` and `.claude/**` are in **no
component's selectors** — the repo's own governance surface is unowned, so a
change to it maps to no component and inherits no invariants. This advance
declares `infra` and should extend that component's selectors to cover them,
or record why a separate `governance` component would be better. Noted rather
than quietly ignored, because "which component owns this?" is a question
ARRIVE exists to make answerable.

## Risk + Rollback

- Risk: a discipline nobody follows is worse than none, because it implies a
  coverage that does not exist. Keep v1 to the four rules above; let the
  observation log add, not the author's imagination.
- Risk: rules that are always-applied consume context in every session. The
  rule stays short and defers detail to the skill precisely for this.
- Risk: generated files drift from their canonical source if someone
  hand-edits `CLAUDE.md` or a `.mdc`. The generated headers already warn;
  the advance verifies the sync output rather than editing it.
- Rollback: revert branch and re-run sync; the harnesses lose the rule and
  keep working exactly as they do today.

## Evidence

- [ ] tests:unit — n/a for authored rules; the check is that `arrive sync
      agent-rules` reproduces the committed outputs with no diff
- [ ] sync:verified — generated Claude, Cursor and skill outputs inspected
      and committed
- [ ] connection:documented — each harness's path recorded, with unverified
      ones labelled

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7.

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation, grounded in the files actually
changed)
