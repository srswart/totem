---
advance:
  id: "ADV-GATEWAY-005"
  title: "Investigation: rmcp maturity + cloud-harness remote MCP reach"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-05T11:12:00Z"
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 77
  risk_flags: ["concurrency"]
  evidence: ["profile:selected-practices", "investigation:findings", "tests:integration"]
  practices:
    tidy_first:
      status: not_applicable
      rationale: "Investigation mode, no production_code work product — crates/totem-mcp-spike is throwaway evidence ADV-GATEWAY-002 supersedes, and there was no existing code to prepare."
    tdd:
      status: not_applicable
      rationale: "Investigation mode, no production_code work product. The Echo server and its round-trip tests were built together, iterated against real compiler and runtime feedback (see Changes Made), not a red phase ahead of an implementation. No tdd:red-green is claimed."
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software]
  work_products: []
  status: complete
---

## Objective

Close two Solution Intent open items before gateway implementation: §7's
"verify current state of the Rust MCP SDK (`rmcp`) at implementation time"
and §9's "which cloud agents can actually attach remote MCP today". Spike a
minimal rmcp server (stdio + streamable HTTP) and test attachment from each
target harness: Claude Code, Cursor (local + background agents), Anthropic
cloud sessions.

## Outcome

A per-harness capability matrix and a `rmcp =3.1.0` go/no-go are recorded in
[docs/tech-direction/mcp.md](../../../../docs/tech-direction/mcp.md).
`rmcp`'s stdio and streamable-HTTP transports were both spiked and exercised
end-to-end by a real client (not hand-rolled JSON-RPC) in
`crates/totem-mcp-spike`. The "standard drift" risk from the brief has a
concrete, date-stamped baseline: `rmcp` shipped three breaking major versions
in five months, the latest five days before this investigation ran.

**A factual correction to this advance's own Objective/Planned Work:**
"test attachment from each target harness: Claude Code, Cursor (local +
background agents), Anthropic cloud sessions" implied live interactive
attachment testing against each real application. That is not something a
headless cloud sandbox running this investigation can do — there is no
Cursor instance, no Claude Desktop instance, and no separate harness to
attach *from* here. What was actually done instead, per harness:

- **Claude Code (desktop + web sessions):** primary-source documentation
  (`code.claude.com/docs/en/mcp`, fetched live) plus **direct
  self-observation** — the session running this investigation *is* a Claude
  Code web/cloud session with live MCP connections at time of writing. That
  is real evidence, not a documentation claim, for this one harness.
- **Anthropic cloud agents via the Claude API MCP connector:** primary-source
  documentation (`platform.claude.com/docs/en/agents-and-tools/mcp-connector`,
  fetched live).
- **Cursor (local + background agents):** genuinely **not verified**.
  `docs.cursor.com` is blocked by this sandbox's egress policy (`CONNECT
  tunnel failed, response 403`), the same class of block prior investigations
  (ADV-STORE-003/004/006) hit for other hosts. What's recorded for Cursor is
  sourced from `WebSearch` summaries of third-party setup guides, explicitly
  labeled as unverified secondary sources in the findings doc — not
  independently confirmed against Cursor's own documentation or a real
  session.

This gap is real and carried forward to `ADV-GATEWAY-003`, not papered over
with confident-sounding prose (see Risk + Rollback).

## Planned Work

- [x] branch: create or confirm feature branch for this advance
- [x] spike: minimal rmcp echo server over stdio and streamable HTTP —
      `crates/totem-mcp-spike`, one `Echo` tool server, both transports,
      both exercised by real rmcp client round-trip tests
- [x] test attachment from Claude Code, Cursor local, Cursor background,
      Anthropic cloud — **partially fulfilled as literally stated; see the
      correction in Outcome.** Claude Code and the Claude API MCP connector
      are backed by primary documentation plus, for Claude Code web
      sessions, direct self-observation. Cursor (local and background) is
      unverified — documentation is blocked in this sandbox and no live
      Cursor session is available here.
- [x] write capability matrix + SDK recommendation to
      docs/tech-direction/mcp.md

## Bug Fixes

- [ ] None — no defect in existing repo code.

## Risk + Rollback

- Risk (open): **Cursor's remote/background-agent MCP reach is unverified**,
  not merely undocumented — the sandbox's egress policy blocks the primary
  source (`docs.cursor.com`), so `ADV-GATEWAY-003`'s least-privilege
  cloud-token design should either re-run this check from a reachable
  environment or a real Cursor session, or explicitly accept it as an
  unconfirmed assumption rather than a verified capability.
- Risk (realised, contained): harness capabilities change fast, and `rmcp`
  itself moves fast (MCP-002 in the findings doc: three major versions in
  five months). The matrix and the pin are both date-stamped 2026-08-05;
  `ADV-GATEWAY-002` should re-verify the `rmcp` pin, not assume it is still
  current, per Solution Intent §7's own "verify at implementation time"
  instruction.
- Rollback: findings only. `crates/totem-mcp-spike` is an isolated workspace
  member with no dependents; deleting it and its workspace entry removes the
  whole change. `totem-gateway` will implement its own MCP integration
  rather than importing this spike.

## Evidence

- [x] profile:selected-practices — investigation mode, empty `work_products`;
      `tidy_first` and `tdd` recorded `not_applicable` with rationale in
      frontmatter. No `tdd:red-green` claimed.
- [x] investigation:findings —
      docs/tech-direction/mcp.md, MCP-001…MCP-005, each tied to an executed
      spike, a fetched primary source, a direct self-observation, or an
      explicitly-labelled secondary source.
- [x] tests:integration — `cargo test -p totem-mcp-spike`: 2/2 pass, both
      real rmcp-client-to-rmcp-server round trips (stdio child process;
      streamable HTTP on an OS-assigned loopback port), no mocking, no
      external network dependency.
- Manual wire-level check: raw `curl` JSON-RPC `initialize` against the
  running `echo_streamhttp` binary, output captured verbatim in the findings
  doc.
- Not claimed: `ci:passed` — no pipeline result was observed for this branch.
- Not claimed: any live-attachment test result for Claude Code, Cursor, or
  the Claude API connector as an actual interactive session in this sandbox
  — see the Outcome correction above for exactly what each harness row rests
  on instead.

## CI Evidence Notes

- Findings document is the primary artifact; no pipeline evidence expected.
- Checks run locally on the branch: `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `arrive doctor artifacts`, `arrive plan check`, `arrive check --strict`,
  `arrive score`.

## Changes Made

### 2026-08-05 - feat: add the ADV-GATEWAY-005 rmcp investigation spike
- Cargo.toml: added `crates/totem-mcp-spike` as a workspace member
- Cargo.lock: regenerated for `rmcp =3.1.0` (exact pin) and its transitive
  dependencies (axum 0.8.9, tokio 1.53.1, reqwest 0.13.4, schemars 1.2.2, and
  others pulled in by the `client`, `transport-io`,
  `transport-child-process`, `transport-streamable-http-server`, and
  `transport-streamable-http-client-reqwest` features)
- crates/totem-mcp-spike/Cargo.toml: pinned `rmcp =3.1.0` exactly (same
  precedent as `surrealdb`/`ureq` in ADV-STORE-004/003 — this spike's
  findings are version-specific and hourly cloud runs must not break because
  an upstream release changes the API mid-project); two `[[bin]]` targets
  instead of `[[example]]` so integration tests can launch the stdio server
  via `env!("CARGO_BIN_EXE_echo_stdio")`
- crates/totem-mcp-spike/src/lib.rs: `Echo`, one `#[tool_router(server_handler)]`
  tool (`echo(text) -> text`), shared by both transports
- crates/totem-mcp-spike/src/bin/echo_stdio.rs: serves `Echo` over `stdio()`
- crates/totem-mcp-spike/src/bin/echo_streamhttp.rs: mounts `Echo` via
  `StreamableHttpService` on a real `axum::Router` at `/mcp`
- crates/totem-mcp-spike/tests/stdio_roundtrip.rs: spawns the compiled
  `echo_stdio` binary as a real child process (`TokioChildProcess`), connects
  a real rmcp client, calls `list_tools` + `call_tool("echo", ...)`, asserts
  the round trip
- crates/totem-mcp-spike/tests/streamhttp_roundtrip.rs: binds a real
  `axum::Router` to an OS-assigned loopback port, connects a real rmcp
  client over `StreamableHttpClientTransport`, asserts the same round trip
  over actual HTTP
- docs/tech-direction/mcp.md: new — verdict, MCP-001…MCP-005, the per-harness
  capability matrix, recommendation, and residual risk (Cursor unverified)

  During development, the first version of both test files used
  `CallToolRequestParam` (singular) — the README's prose example implies
  this name but the actual 3.1.0 type is the plural
  `CallToolRequestParams::new(name).with_arguments(...)` builder. `cargo
  build` caught this immediately (`E0432: unresolved import`, with a
  "similar name exists" suggestion); fixed before the tests were ever run,
  so the committed tests reflect the real compiled-and-passing API, not the
  README's prose shorthand. Not split into a separate commit because the
  broken version was never itself committed.

### 2026-08-05 - docs: complete ADV-GATEWAY-005
- arrive/systems/058-totem-core/advances/ADV-GATEWAY-005.md: status complete,
  evidence, practice dispositions, the Objective/Planned-Work correction on
  what "test attachment" actually meant in this sandbox, refreshed CFU
- arrive/implementation-plan.yaml: plan item ADV-GATEWAY-005 set to done

## Reviewability

`arrive score` reports **77 [RED]** (size 52, novelty 20, risk 5) against
`origin/master`. The change is kept whole rather than split.

The `concurrency` risk flag is accurate here, not a heuristic false positive
like ADV-STORE-003's: `tests/streamhttp_roundtrip.rs` genuinely spawns a
`tokio::spawn` task to run the axum server alongside the test's client code,
and both binaries are async `tokio::main` programs. Left in place, not
disputed.

Of 1033 changed lines, 357 are the generated `Cargo.lock` delta for one new
pinned dependency (`rmcp`) and its transitive tree — not hand-written:

| File | Lines | Splittable? |
|---|---|---|
| `docs/tech-direction/mcp.md` | 275 | No — the findings, capability matrix, and the correction to this advance's own Objective are the deliverable this investigation exists to produce |
| `arrive/.../ADV-GATEWAY-005.md` | 231 | No — this record, required by the same advance |
| `crates/totem-mcp-spike/tests/streamhttp_roundtrip.rs` | 66 | No — the one HTTP round-trip test the streamable-HTTP half of MCP-001 rests on |
| `crates/totem-mcp-spike/tests/stdio_roundtrip.rs` | 47 | No — the one stdio round-trip test the other half of MCP-001 rests on |
| `crates/totem-mcp-spike/Cargo.toml` | 46 | No — one workspace-member addition, one pinned dependency, two `[[bin]]` targets both tests depend on |
| `crates/totem-mcp-spike/src/{lib,bin/*}.rs` | 61 | No — one `Echo` server shared by both transports; splitting stdio from streamable-HTTP would leave either test unable to compile |
| `Cargo.toml`, `Cargo.lock` | 371 | No — one workspace-member addition, one pinned dependency's transitive tree |

Splitting by transport (stdio vs. streamable HTTP) would land two advances
each proving only half of MCP-001 and unable to support the capability
matrix's side-by-side comparison; splitting the findings doc from the code
would land either prose with nothing a reviewer can run to reproduce
MCP-001, or code with no record of what was found. As with
ADV-STORE-003/004, a reviewer can read `docs/tech-direction/mcp.md` first
and treat `crates/totem-mcp-spike` as its appendix.

## Changes Made (continued)

### 2026-08-05 - fix: address PR #6 review comments from Copilot

Three points raised on PR #6, all accepted:

- `docs/tech-direction/mcp.md`: the MCP-001 bullet read as if `nest_service`
  chains onto `StreamableHttpService::new(...)` itself. Reworded — the spike
  code calls `axum::Router::new().nest_service("/mcp", service)`;
  `StreamableHttpService` is what gets mounted, not what does the mounting.
- `crates/totem-mcp-spike/tests/streamhttp_roundtrip.rs`: the spawned axum
  server task (`tokio::spawn`) was aborted at the end but the resulting
  `JoinHandle` was never awaited, so a real panic or error from
  `axum::serve(...)` occurring before the abort would have been silently
  dropped along with the handle. Now awaits the handle after `abort()` and
  panics on anything that isn't a plain `is_cancelled()` result.
- `arrive/systems/058-totem-core/advances/ADV-GATEWAY-005.md` frontmatter:
  `reviewability_score` and `risk_flags` were left at their template
  defaults (`0`, `[]`) even though the `## Reviewability` section already
  documented the real `arrive score` result (77 RED, `concurrency` flag).
  Frontmatter now matches: `reviewability_score: 77`, `risk_flags:
  ["concurrency"]` — the score as measured when this advance was completed,
  not re-measured after this smaller follow-up fix (whose own incremental
  diff scores 2 GREEN against the prior push, a different, narrower
  measurement than "the advance's" score).

## Check for Understanding

1. `tests/streamhttp_roundtrip.rs` binds `127.0.0.1:0` and reads back the
   OS-assigned port before connecting. Why does this matter for an hourly
   cloud routine specifically, compared to the fixed `127.0.0.1:8765` that
   `src/bin/echo_streamhttp.rs` uses?
2. The manual `curl` check in MCP-001 got back `text/event-stream`-framed
   output (`data: {...}`) despite `Accept: application/json,
   text/event-stream` being sent and `StreamableHttpServerConfig::default()`
   being used server-side. What specific config call would change that, per
   the finding, and why doesn't sending `Accept: application/json` alone
   change the server's behavior?
3. MCP-003's evidence for Claude Code web sessions is described as
   "self-observed," distinct from MCP-004's evidence for the Claude API
   connector. Point to the specific sentence in the findings doc that marks
   this distinction, and explain why it matters for how much confidence a
   reader should place in each row of the capability matrix.
4. The advance's own original Objective asked to "test attachment from...
   Cursor (local + background agents)." What did this advance actually do
   instead for that specific harness, and where in this file is the
   discrepancy between what was asked and what was possible recorded?
5. `rmcp` is pinned `=3.1.0` in `crates/totem-mcp-spike/Cargo.toml`. Using
   MCP-002's own version-history evidence, explain why "verify current state
   of the Rust MCP SDK at implementation time" (Solution Intent §7) is not
   satisfied once by this advance, and name the specific downstream advance
   responsible for re-checking it.
6. `frontmatter.practices.tdd` is `not_applicable`, yet
   `tests/stdio_roundtrip.rs` and `tests/streamhttp_roundtrip.rs` were
   written and iterated against a real compiler error
   (`CallToolRequestParam` vs. `CallToolRequestParams`). What distinguishes
   this from `tdd:red-green`, and where in the Changes Made log is that
   distinction made explicit?
7. Solution Intent §9's open question was "which cloud agents can actually
   attach remote MCP today." Name the one harness in the capability matrix
   whose row this advance leaves marked "unverified" rather than
   "documented" or "self-observed," and what advance is asked to close that
   gap.
8. After the PR #6 review fix, `tests/streamhttp_roundtrip.rs` awaits the
   spawned server's `JoinHandle` after calling `abort()` instead of dropping
   it. What specific failure mode does this close, and why does the fix
   check `e.is_cancelled()` rather than treating every `Err` from that await
   as a test failure?
9. `reviewability_score: 77` in the frontmatter was set to match the score
   already documented in `## Reviewability`, not re-measured after the
   follow-up `fix:` commit (whose own diff separately scores 2 GREEN). What
   is the difference in what those two numbers actually measure, and why
   does the frontmatter carry the first one rather than the second?
