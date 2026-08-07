---
advance:
  id: "ADV-GATEWAY-015"
  title: "MCP tool calls acknowledge, or fail — never neither"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-07T20:00:00Z"
  implementation_completed_at: "2026-08-07T22:15:00Z"
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 25
  risk_flags: ["concurrency"]
  evidence: ["tests:integration", "investigation:findings"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
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

- [x] branch / claim
- [x] reproduce — **attempted and failed to reproduce**; see below
- [x] tidy: none needed
- [x] test: client-shaped HTTP tests asserting every tool call terminates
- [x] fix: the defects the investigation actually found

## Outcome: not reproduced, and that is the finding

Three tests drive a **real rmcp client over real HTTP** against the
authenticated application — a save, a recall, and three consecutive saves on
one session, each with a ten-second patience. All acknowledge. Against the
deployed gateway, `curl` also receives a complete response in under a second
and the stream closes.

So the server terminates its responses correctly for a well-behaved MCP
client. The advance explicitly permitted this outcome: *"If the cause proves
to be client-side rather than ours, that is recorded as the finding and the
advance closes honestly rather than inventing a server-side change to justify
itself."* No server-side change was invented.

**What remains unexplained:** why claude.ai's connector waited. The most
plausible mechanism, established below, is that responses are SSE-framed and
carry an SSE `retry` directive, which invites reconnect semantics in a client
that treats the stream as long-lived. That is a hypothesis, not a diagnosis —
it was not confirmed, and saying so is the point.

## Two defects the investigation did find

**1. `json_response = true` is dead configuration.** rmcp 3.1.0 consults it on
the *stateless* paths only (`tower.rs` 1255, 1983). Once a client holds a
session — every client, immediately after `initialize`, because the service is
mounted with a `LocalSessionManager` — a POSTed request returns
`sse_stream_response(..)` unconditionally. The comment beside the setting
claimed Totem answers tool calls with plain JSON. It does not. The claim
survived because **no test drove the transport**; the comment is now accurate,
and the setting is kept because it is correct for the session-less callers it
does reach.

**2. The gateway compiled two HTTP stacks.** rmcp pulls `reqwest 0.13.4`; the
OAuth work (ADV-GATEWAY-013) added `reqwest 0.12`. The TLS feature rename
between them (`rustls-tls` → `rustls`) is what hid the duplication — the same
rename that bit ADV-CLI-002 from the other direction. Aligned to 0.13.

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

## Reviewability

`arrive score --base origin/advance/phase-011`: **25 [GREEN]**.

## Evidence

- [x] tests:integration — `crates/totem-gateway/tests/mcp_http_acknowledgement.rs`:
      three tests driving a real rmcp client over real HTTP, each bounded by a
      timeout so a hang fails rather than hangs the suite. This crate had
      **no transport-level coverage at all** before now, which is the same
      blind spot that produced the published-schema bug (ADV-GATEWAY-013) and
      the CLI's missing TLS (ADV-CLI-002). The tests are the durable outcome
      of this advance, more than any fix.
- [x] investigation:findings — the two defects above, with the rmcp line
      numbers that establish the first.
- [ ] **Not claimed: a reproduction, a diagnosis, or a fix for the reported
      hang.** It did not reproduce. Recording a hypothesis as a diagnosis
      would be exactly the kind of confident-sounding wrongness this
      project's honesty rules exist to prevent.

## What would advance this if it recurs

- Capture the client side: whether claude.ai reconnects to the SSE stream
  after the response, and whether it is waiting on the POST response or the
  standalone GET stream.
- Compare a session-less call (which *does* get plain JSON) against a
  session-bearing one from the same client.
- Rule Fly's proxy in or out by reproducing against the deployment with the
  rmcp client rather than only on loopback.

## Changes Made

### 2026-08-07 - test: [ADV-GATEWAY-015] client-shaped HTTP tests; correct a false claim
- crates/totem-gateway/tests/mcp_http_acknowledgement.rs: new — three
  timeout-bounded tests over a real HTTP transport
- crates/totem-gateway/src/mcp_http.rs: the `json_response` comment now says
  what the setting actually does
- crates/totem-gateway/Cargo.toml: reqwest aligned to rmcp's 0.13

## Check for Understanding

1. This advance closed without fixing the bug it was written for. What did
   its own Behavioral Change section permit, and why is that permission more
   valuable than a change that made the advance look productive?
2. `config.json_response = true` had no effect on any real client. Which
   condition decides whether rmcp consults it, and what does mounting a
   `LocalSessionManager` guarantee about that condition?
3. The comment beside that setting was wrong for as long as it existed. What
   kind of test would have caught it, and why did 66 passing tests not?
4. Two reqwest versions were compiled into one binary. What made the
   duplication easy to miss, and where had the same detail already caused
   trouble in a different crate?
5. The advance lists three things that would advance the investigation if the
   hang recurs. Which of them distinguishes a client-side cause from a
   Fly-proxy cause, and why can loopback tests not settle that question?
