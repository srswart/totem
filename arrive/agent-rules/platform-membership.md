# Platform Membership

This repository is a **member** of a Platform ARRIVE cluster (see the generated
`arrive-platform` rule for the platform's identity, checkout path, and **adopted
capabilities**). Platform coordination is **opt-in per capability** — teams may
start with context sharing only and adopt contracts, dispatches, or cross-repo
plan participation when ready.

## Consult platform context before implementing

- At session start, run `arrive platform check` (add `--no-fetch` when offline).
  Resolve "platform missing" or staleness warnings before relying on platform
  context — a stale checkout means stale decisions.
- When **context** is adopted (default): run `arrive context sources` (or
  `--json`) for a single inventory of platform + external sources; read the
  platform INDEX and relevant context artifacts before implementing. Use
  `/arrive-platform-context` when you want a structured brief.
- When **context is not adopted**, do not treat platform artifacts as in-scope
  unless the team enables `platform.adoption.context` in `arrive/registry.yaml`.
- Treat `binding` platform artifacts as constraints (below org policies, above
  repo tech-directions). `advisory` artifacts are strong guidance.

## Contract discipline (when contracts adopted)

- When `platform.adoption.contracts` is enabled and a change touches an
  interface this repo **provides** or **consumes** under a platform contract,
  say so explicitly and propose the acknowledgement bump or a platform contract
  PR. Use `/arrive-contract-impact` when unsure. Never silently drift.
- When contracts are **not** adopted, `arrive platform check` skips contract
  drift for this repo — the platform may still list contracts for other members.

## Capture change upward (when dispatches adopted)

- When `platform.adoption.dispatches` is enabled, **propose**
  `arrive platform publish` at advance completion (propose-and-confirm — never
  publish silently). Optional frontmatter tags `contracts_touched` and
  `plan_items` feed the dispatch when relevant.
- When dispatches are **not** adopted, publication commands and hooks stay
  silent — enable `platform.adoption.dispatches` when the team wants upward
  capture.

## Cross-repo plan (when plan adopted)

- When `platform.adoption.plan` is enabled, consult open plan items assigned to
  this repo in the platform INDEX; tag advances with `plan_items` when you
  progress them.

## Ownership boundary

- **Never edit** files under the platform checkout's `arrive/platform/` from
  this repository. Coordination artifacts are owned by the platform repo —
  propose changes there via PR. The only cross-boundary write that is ever
  legitimate is a dispatch written by `arrive platform publish` (when
  dispatches are adopted).
