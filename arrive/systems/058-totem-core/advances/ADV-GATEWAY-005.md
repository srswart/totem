---
advance:
  id: "ADV-GATEWAY-005"
  title: "Investigation: rmcp maturity + cloud-harness remote MCP reach"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software]
  work_products: []
  status: planned
---

## Objective

Close two Solution Intent open items before gateway implementation: §7's
"verify current state of the Rust MCP SDK (`rmcp`) at implementation time"
and §9's "which cloud agents can actually attach remote MCP today". Spike a
minimal rmcp server (stdio + streamable HTTP) and test attachment from each
target harness: Claude Code, Cursor (local + background agents), Anthropic
cloud sessions.

## Outcome

After this advance:
- A per-harness capability matrix (transport, auth options, quirks) recorded
  for ADV-GATEWAY-002/003 to build against, plus a go/no-go on `rmcp` vs.
  alternatives (hand-rolled protocol layer, other SDK).
- The "standard drift" risk from the brief gets a concrete baseline to track.

## Planned Work

- [ ] branch: create or confirm feature branch for this advance
- [ ] spike: minimal rmcp echo server over stdio and streamable HTTP
- [ ] test attachment from Claude Code, Cursor local, Cursor background, Anthropic cloud
- [ ] write capability matrix + SDK recommendation to docs/tech-direction/mcp.md

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: harness capabilities change fast; date-stamp the matrix and re-verify
  at ADV-GATEWAY-003 time.
- Rollback: n/a — findings only.

## Evidence

- [ ] profile:selected-practices
- [ ] investigation:findings

## CI Evidence Notes

- Findings document is the primary artifact; no pipeline evidence expected.

## Changes Made

- None yet
