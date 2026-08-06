---
advance:
  id: "ADV-CONSOLE-003"
  title: "Live landscape updates: gateway event relay + console subscription"
  system: "058-totem-core"
  primary_component: "console"
  components: ["console", "gateway"]
  started_at: "2026-08-06T09:05:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 40
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["concurrency"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Deliver the live-update behavior that ADV-CONSOLE-001 originally promised and
then honestly descoped: the console's landscape dashboard and memory browser
update automatically when the underlying store changes, without the user
pressing Refresh.

This is the deferred residual recorded in ADV-CONSOLE-001's Risk + Rollback
section. Per TD-009, SurrealDB live queries exist only on embedded/WebSocket
connections, so the console must not open its own DB connection; the gateway
owns the subscription and relays events to the browser.

## Behavioral Change

After this advance:
- `totem-gateway` subscribes to live queries on the landscape (and memory)
  tables and exposes an event stream endpoint (SSE preferred for simplicity;
  WebSocket acceptable if SSE proves insufficient) that forwards change
  notifications to connected browsers.
- The console subscribes to that stream and patches its `landscape`/
  `memories` signals in place; the Refresh button remains as a manual
  fallback but is no longer the only update path.
- The relay endpoint respects the same scope rules as every other read
  surface: events are resolved through store-enforced scope resolution, and
  relayed reads append to the access log (no unlogged access paths).

## Planned Implementation Tasks

- [ ] branch: create or confirm feature branch for this advance
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: gateway relay tests against the embedded in-memory engine with a
      real HTTP/SSE client, mirroring the transport-test pattern from
      ADV-GATEWAY-002; console-side subscription unit tests
- [ ] feat: implement minimal changes to pass tests

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: long-lived streaming connections are the gateway's first stateful
  client surface — connection lifecycle (disconnect, backpressure, slow
  consumers) needs explicit handling or the relay can leak tasks.
- Risk: an event relay that bypasses store scope resolution would become a
  scope-leak vector (the hazard ADV-STORE-001 exists to prevent); events must
  be filtered per-subscriber through store-enforced scope resolution, not
  broadcast then filtered above the store.
- Rollback: the console degrades gracefully to the ADV-CONSOLE-001 behavior
  (manual Refresh) if the stream endpoint is removed or unavailable.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit

## CI Evidence Notes

- If CI jobs are enabled, link pipeline evidence (`ci:passed`) from PR/MR and
  default-branch runs.
- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CONSOLE-003 --status passed`

## Changes Made

- None yet
