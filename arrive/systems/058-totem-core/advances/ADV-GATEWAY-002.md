---
advance:
  id: "ADV-GATEWAY-002"
  title: "MCP server (stdio) exposing recall/save/landscape"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 40
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["public_api", "new_dependency"]
  evidence: []
  model_usage: []
  status: planned
---

## Objective

Serve MCP over stdio for desktop harnesses (Claude Code, Cursor) with the
initial tool surface: `totem_recall`, `totem_save`, `totem_landscape`
(Solution Intent §3.1). Verify the current state of the Rust MCP SDK (`rmcp`)
at implementation time, per §7.

## Behavioral Change

After this advance:
- A desktop harness configured with the Totem MCP server can read merged
  context and write memories in one tool call each — G1's "near-zero setup"
  path for local agents.
- MCP tool calls route through the same core API as REST, so provenance and
  access logging apply identically.

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: tool-dispatch tests over the MCP layer (red first)
- [ ] feat: stdio MCP server with recall/save/landscape tools

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: `rmcp` maturity / MCP spec drift ("standard drift" risk); mitigate by
  keeping the MCP layer a thin adapter over the internal API.
- Rollback: revert branch; REST surface is unaffected.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] tests:integration

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-GATEWAY-002 --status passed`

## Changes Made

- None yet
