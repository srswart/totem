---
advance:
  id: "ADV-GATEWAY-015"
  title: "MCP tool calls acknowledge, or fail — never neither"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-07T20:00:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
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

A `totem_save` through the claude.ai connector **completed server-side while
the client waited indefinitely** (observed 2026-08-07, ADV-GATEWAY-013's
recorded residual). The record was written; the acknowledgement never
arrived.

This is worse than a clean failure. An agent that sees no acknowledgement
retries, and a retried save writes a second copy — so the failure mode is
*silent duplicate memories*, which the curator (ADV-CURATOR-001) later has to
dedupe, in a store whose whole value proposition is that its contents are
trustworthy. It also corrupts the dogfood trial's measurement: recall and
save counts stop meaning what they say.

## Behavioral Change

After this advance:

- Every MCP tool call over streamable HTTP either returns a response or
  fails visibly. A call that mutates state and then strands the client is
  treated as a defect, not a transport quirk.
- The cause is **diagnosed before it is fixed**, and the diagnosis is
  recorded: whether the response is never written, written and not flushed,
  lost to session/`mcp-session-id` handling, or dropped by an intermediary
  (Fly's proxy is in the path and was not in the workstation reproduction).
  A fix applied without knowing which of those it was, is a guess.
- A regression test exercises the tool surface **the way a client does** —
  over HTTP against a running server, not in-process — because in-process
  tests are exactly what missed this (and the schema bug before it).
- If the cause proves to be client-side rather than ours, that is recorded
  as the finding and the advance closes honestly rather than inventing a
  server-side change to justify itself.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] reproduce: a tool call over HTTP that mutates and then hangs; capture
      server logs, response framing, and session headers
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: a client-shaped HTTP test asserting every tool call terminates
- [ ] feat/fix: whatever the diagnosis names

## Scope and Boundaries

**In scope:** acknowledgement and termination of MCP tool calls over
streamable HTTP; the diagnosis; a client-shaped test.

**Out of scope:** idempotency keys for saves (worth considering if
duplicates prove common — a separate decision, not a silent addition here);
the REST surface, which has not shown this.

## Risk + Rollback

- Risk (`concurrency`): response-path changes to a streaming transport are
  easy to get subtly wrong under load, and the failure is invisible in a
  single-caller test.
- Risk: fixing the symptom (a timeout) rather than the cause would convert a
  hang into a *failed* call that has still written a record — the same
  duplicate problem with better manners. The advance requires the diagnosis
  first for exactly this reason.
- Rollback: revert branch; behaviour returns to the current hang.

## Evidence

- [ ] reproduction: captured, with logs and framing verbatim
- [ ] diagnosis: which of the candidate causes it actually was
- [ ] tests:integration — a client-shaped HTTP test that fails on the old
      behaviour

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
