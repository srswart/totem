# ARRIVE Core

Outcome-first; `/arrive/` is authoritative. Do not bypass governance.

`governance_preset`: Full default; Lite keeps binding rules.

## Authoritative Metadata

- `/arrive/` is the governance source — understand impact before edits.
- Major decisions: reference `docs/tech-direction/`.

## Before Implementing or Editing

1. Impacted **system(s)** — `system.yaml` roots; **component(s)** — selectors.
2. **Resident** touched? → `/arrive-resident-check` (gate detail there).
3. **Platform member?** → `arrive platform check` + platform INDEX (`arrive-platform-membership`).
4. On "implement ADV-XXX": always-applied `arrive-rules` are **binding** (blocking/mandatory); surface or waive what you can't satisfy; then `arrive check` / `arrive advance attest`.

## AI Template-First Authoring

Use `arrive template render --kind <system|component|advance|implementation-plan> --json`
content as the authoring base.

## Optional Implementation Plan

When `arrive/implementation-plan.yaml` exists: `arrive plan show` / `arrive plan start`; keep plan + advances aligned (`.arrive/work-context.json` = local focus).

## Reviewability Budget

- **Green** (≤30) · **Yellow** (31–60) · **Red** (>60, split unless documented)
- One reviewable Advance beats one mega-diff; `arrive score` (detail in `arrive-reviewability`)
- **tidy → test → implement** (detail in `arrive-dev-practices`)

## CI Guidance

PR/MR checks required even if pipeline YAML is disabled; run externally (`arrive pr check --strict`, `arrive evidence record`).

## CFU

Every advance: keep `## Check for Understanding` aligned on substantive updates (see `arrive-advance-writing`).

## Transparency on Large Changes

Before large edits: state diff footprint (~files/LOC, impacted components).

## Scoped rule map

Detail in: `arrive-dev-practices` (tidy/TDD), `arrive-reviewability`, `arrive-advance-writing` (advances/CFU), `arrive-artifacts` (YAML).
