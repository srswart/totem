---
advance:
  id: "ADV-INFRA-004"
  title: "Memory discipline: standalone rules and a Totem skill (trial v1)"
  system: "058-totem-core"
  primary_component: "infra"
  components: ["infra"]
  started_at: "2026-08-07T19:00:00Z"
  implementation_completed_at: "2026-08-07T19:30:00Z"
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: ["rules:authored", "connection:executed"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

Give every harness that works on this repo the rules and the on-demand detail
for *using* Totem, so the trial can start capturing and using memories — and
learn what discipline actually works before deciding how it should be
packaged.

**Direction set by Shawn 2026-08-07, replacing this advance's original
premise.** The first draft routed everything through `arrive/agent-rules/`
and `arrive sync agent-rules`. That is the repo's cross-harness pipeline and
it works — but ARRIVE and Totem are separate products that install,
initialise and enrol independently, and baking Totem's usage rules into the
ARRIVE kit presumes an integration decision this trial has not yet earned.
So: **author standalone, experiment, settle packaging afterwards.**

What that changes is *where the files live*, not what they say. The rules are
hand-authored and committed rather than generated, and named so an
`arrive sync agent-rules` run cannot clobber them.

## Behavioral Change

After this advance:

- **`CLAUDE.local.md`** carries the four always-applied rules for workstation
  Claude Code. ARRIVE never writes this file — which is exactly why it suits
  an independent ruleset — and it is *not* gitignored, so unlike most "local"
  files it travels with a clone.
- **`.claude/skills/totem-memory/SKILL.md`** carries the detail: the two
  moments that matter, what is worth saving, **what never to save**, the six
  categories, the scope model, tool shapes, and per-harness connection
  instructions. Loaded on demand, so it costs nothing until needed.
- **`.cursor/rules/totem-memory.mdc`** gives Cursor the same four rules,
  hand-authored under a name no ARRIVE sync generates, and states plainly
  that Cursor's remote-MCP reach is **unverified** — a reader in Cursor
  without Totem tools is meeting a known gap, not a misconfiguration.
- **`docs/cloud-agent-notes.md` gains Step 2.5 and Step 9.5**: recall before
  reading the governance docs, and save what the next run needs before the
  journey report. A missing connector is explicitly *a finding to report*,
  not a reason to stop.
- **The workstation is connected**: `claude mcp add --transport http` against
  the deployed gateway, verified `✔ Connected`.

## The discipline itself (v1 hypothesis)

Four rules, deliberately few, because always-applied rules cost context in
every session:

1. Recall *before* reading the governance docs — a memory that contradicts a
   doc is the most valuable thing a session can find, and reading the doc
   first frames it away.
2. Save what the next session needs, not what you did: decisions and their
   reasoning, **dead ends**, spec corrections, constraints found by running
   something.
3. Default to `project:` scope. Too narrow is invisible, too wide is noise,
   promotion is the sanctioned path between.
4. Feedback when a memory helped or misled — the value loop has no other
   signal.

The skill names dead ends as the highest-value and most-skipped category,
because finishing something feels more recordable than failing at it. Whether
that holds is exactly what the trial measures.

## Planned Implementation Tasks

- [x] branch / claim
- [x] author the four rules (CLAUDE.local.md, Cursor rule) and the
      `totem-memory` skill
- [x] cloud protocol: Step 2.5 recall, Step 9.5 save
- [x] connect the workstation; verify
- [ ] connect the cloud routines — claude.ai configuration rather than repo
      content; see Evidence
- [x] record the packaging decision as deferred

## Scope and Boundaries

**In scope:** the rules, the skill, the protocol steps, the workstation
connection, and recording that packaging is deferred.

**Out of scope:** credentials and measurement (ADV-INFRA-003); CLI
authentication (ADV-CLI-002); whether the discipline is any good, which only
running it will tell.

## A component-ownership gap this exposes

`arrive/agent-rules/**`, `.cursor/**` and `.claude/**` are in **no
component's selectors** — the repo's own governance surface is unowned, so a
change to it maps to no component and inherits no invariants. This advance
declares `infra` because it had to declare something. Extending `infra`'s
selectors, or adding a `governance` component, is a real decision left open
rather than quietly resolved by an advance that happened to touch these
paths.

## Risk + Rollback

- Risk: a discipline nobody follows is worse than none, because it implies a
  coverage that does not exist. v1 is four rules; the observation log adds,
  not the author's imagination.
- Risk: always-applied rules consume context in every session. The rule stays
  short and defers to the skill precisely for this.
- Risk: an `arrive sync agent-rules` run overwriting hand-authored files.
  Mitigated by naming — nothing in the kit generates `totem-memory` — and
  `CLAUDE.local.md` is a file ARRIVE contractually never writes.
- Rollback: delete the three files and revert the two protocol steps; the
  harnesses lose the rules and keep working exactly as today.

## Evidence

- [x] rules:authored — `CLAUDE.local.md`,
      `.cursor/rules/totem-memory.mdc`,
      `.claude/skills/totem-memory/SKILL.md`, and the two protocol steps in
      `docs/cloud-agent-notes.md`. No code changed; the workspace suite is
      unaffected.
- [x] connection:executed — workstation Claude Code registered against
      `https://totem-dev.fly.dev/mcp`, reporting `✔ Connected`. A session
      started after registration has the tools.
- [ ] **Not done: the cloud routines are not attached.** The connector exists
      in claude.ai and authenticates, but attaching it to
      `totem-advance-opus` and `totem-advance-sonnet` is configuration this
      advance did not perform. Until then Step 2.5 will correctly report a
      missing connector — anticipated by the step, but it means the cloud
      half of the trial has not begun.
- Not claimed: that the discipline is any good. It is a hypothesis, and the
      observation log is where it earns or loses its place.

## CI Evidence Notes

- No code changed; the standing checks apply unchanged.

## Changes Made

### 2026-08-07 - feat: [ADV-INFRA-004] Totem usage rules and skill
- CLAUDE.local.md: new — four always-applied workstation rules, committed
  and ARRIVE-safe
- .claude/skills/totem-memory/SKILL.md: new — the detail, loaded on demand
- .cursor/rules/totem-memory.mdc: new — the same rules for Cursor, with the
  unverified-reach caveat stated
- docs/cloud-agent-notes.md: Step 2.5 (recall first) and Step 9.5 (save what
  the next run needs)

## Check for Understanding

1. The rules live in `CLAUDE.local.md` rather than `arrive/agent-rules/`.
   What property of that file makes it suitable for an *independent* ruleset,
   and what second property (which most "local" files lack) makes it usable
   by a second developer?
2. Four rules are always-applied and everything else sits in a skill. What
   does that split cost, and what does it buy, on a session that never
   touches Totem at all?
3. The skill calls dead ends the highest-value and most-skipped memory
   category. Why would an agent systematically under-record them, and what in
   the rule text is meant to counteract that?
4. Step 2.5 puts recall *before* reading the governance docs rather than
   after. What failure does that ordering prevent, and what does it ask a run
   to do when a memory and a document disagree?
5. The Cursor rule states that Cursor's remote-MCP reach is unverified. Which
   investigation left it unverified and why, and what would it take to turn
   that line into a claim?
