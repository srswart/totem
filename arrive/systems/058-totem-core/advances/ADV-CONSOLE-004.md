---
advance:
  id: "ADV-CONSOLE-004"
  title: "Console visual design system (Tailwind, workstation)"
  system: "058-totem-core"
  primary_component: "console"
  components: ["console"]
  started_at: "2026-08-06T13:45:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 35
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

The console currently renders as unstyled browser-default HTML (ADV-CONSOLE-001
screenshots, 2026-08-06). Give it a coherent visual design system so the
first "humans observe" surface looks like a product: layout, navigation,
readable data presentation, and clear status communication — without
compromising the thin-view architecture (`view_model.rs` parses, `app.rs`
renders) that keeps the console testable.

WORKSTATION advance: Tailwind's CLI is a downloaded binary the cloud
sandbox's egress proxy would likely refuse (the SurrealDB and embedding-model
precedents), and visual design is verified by a human looking at a browser —
both point at a workstation session. Cloud routines skip it per the
workstation gate.

## Approach

**Tailwind CSS v4 via Dioxus's native integration** — `dx` 0.7 detects a
Tailwind input stylesheet and runs the Tailwind CLI during
`dx serve`/`dx build`, so utility classes in `rsx!` are the styling surface
and no JS toolchain enters the workspace. Pin the Tailwind CLI version so
the toolchain stays reproducible.

## Behavioral Change

After this advance:

- The console has an app shell: header with the repo/actor/project identity
  controls, tab navigation that reads as tabs, and a content area with
  consistent spacing and type hierarchy.
- The landscape view presents systems/components/advances as structured
  cards or tables — advance status (`planned` / `in_progress` / `complete` /
  `done`) rendered as colored status badges, component stage and owners
  legible at a glance.
- The memory browser groups categories with clear headings; each memory
  shows scope, tags, and body with visual hierarchy rather than a bullet
  line. Empty states ("no memories in view for this scope chain") look
  intentional, not broken.
- Refresh/loading states are visible (the fetch already exists; show it).
- `dioxus-ssr` render tests updated: assert the semantic structure and
  status-badge classes, not pixel styling.
- Verified in a real browser (`dx serve`), with before/after screenshots
  recorded as evidence — the verification ADV-CONSOLE-001 had to defer is
  in scope here by construction.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: SSR structure/badge assertions ahead of the restyle
- [ ] feat: app shell, landscape cards/badges, memory browser hierarchy,
      empty/loading states

## Bug Fixes

- [ ] None yet

## Scope and Boundaries

**In scope:** visual design of the two existing views and the shell; the
styling toolchain decision; SSR test updates.

**Out of scope:** new data or routes (the relay is ADV-CONSOLE-003; audit
views are ADV-CONSOLE-002); dark mode and theming beyond the token palette;
(browser verification is IN scope for this advance — see Behavioral
Change).

## Risk + Rollback

- Risk (`new_dependency` flag): the Tailwind CLI enters the dev toolchain
  (not the crate graph). Pin its version in `Dioxus.toml`/config so hourly
  runs aren't broken by an upstream release, mirroring the surrealdb pin
  rationale.
- Risk: a restyle that rewrites `app.rs` structure can silently change
  rendered *content*; the tests-before-restyle task exists so the SSR
  assertions catch dropped data.
- Risk: wasm binary size growth from styling is negligible (classes +
  stylesheet), but record the before/after `dx build` size in evidence.
- Rollback: revert branch; the console returns to unstyled HTML, fully
  functional.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] screenshots: before/after, recorded in the advance

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7;
  `cargo check/clippy --target wasm32-unknown-unknown -p totem-console`
  remain the closest cloud-reachable verification, per ADV-CONSOLE-001.

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation, grounded in the files actually
changed)
