# Tech Direction: MCP SDK + per-harness remote reach

**Status:** Findings accepted, partially closed · **Date:** 2026-08-05 ·
**Advance:** ADV-GATEWAY-005 · **Companion:**
[solution-intent.md](../solution-intent.md) §3.1, §7, §9 · to be built on by
`ADV-GATEWAY-002` (stdio MCP server) and `ADV-GATEWAY-003` (streamable-HTTP +
auth for cloud agents), both currently `status: planned`

Solution Intent §7 says "MCP via the official Rust SDK (`rmcp`) — verify
current state at implementation time"; §9 leaves open "which cloud agents can
actually attach remote MCP today". This document records what a spike could
execute in this environment, and what could only be sourced from
documentation, for each.

**Verdict: go on `rmcp` — pin `=3.1.0`, re-verify before ADV-GATEWAY-002.**
Both transports Totem's gateway needs (stdio for desktop harnesses,
streamable HTTP for cloud ones, §3.1) were spiked and executed end-to-end in
this sandbox with a real client, a real server, and zero hand-rolled
JSON-RPC. The harness capability matrix (§3) is real for Claude Code and the
Claude API MCP connector — both have primary-source documentation reachable
from this sandbox — but Cursor's remote/background-agent reach could **not**
be independently verified: `docs.cursor.com` is blocked by this sandbox's
egress policy, the same class of block ADV-STORE-003/004/006 hit for other
hosts. That gap should be closed with a reachable environment (or an actual
Cursor session) before ADV-GATEWAY-003 finalizes cloud-harness assumptions.

## 1. What was run

| | |
|---|---|
| Code | `crates/totem-mcp-spike` — one `Echo` tool server, two transports, two client round-trip tests |
| Stdio | `src/bin/echo_stdio.rs` (server) + `tests/stdio_roundtrip.rs` (real rmcp client over `TokioChildProcess`) |
| Streamable HTTP | `src/bin/echo_streamhttp.rs` (server, mounted on a real `axum::Router`) + `tests/streamhttp_roundtrip.rs` (real rmcp client over `StreamableHttpClientTransport`, OS-assigned loopback port) |
| Test run | `cargo test -p totem-mcp-spike` (2 tests, both real client↔server round trips, no mocking) |
| Manual wire check | `curl` a raw JSON-RPC `initialize` at the running `echo_streamhttp` binary |

## 2. The load-bearing findings

### MCP-001 — `rmcp` 3.1.0's stdio and streamable-HTTP transports both work end-to-end, server and client, against the exact feature-gated API Totem's gateway needs. *(confirmed, by execution)*

`Echo` (one `#[tool_router(server_handler)]` impl, one `echo` tool) is served
unmodified over both transports:

- `tests/stdio_roundtrip.rs` spawns the compiled `echo_stdio` binary as a
  real child process via `TokioChildProcess`, connects a real rmcp client
  over its stdin/stdout, calls `list_tools` and `call_tool("echo", ...)`, and
  asserts the echoed text comes back unchanged.
- `tests/streamhttp_roundtrip.rs` binds a real `axum::Router` (via
  `StreamableHttpService::new(...).nest_service("/mcp", ...)`) to an
  OS-assigned loopback port, connects a real rmcp client over
  `StreamableHttpClientTransport::from_uri(...)`, and asserts the same round
  trip over actual HTTP.

Both tests passed on the first API shape the compiler accepted after two
small corrections (`CallToolRequestParams`, not the singular
`CallToolRequestParam` the README's prose implied; see Changes Made) — the
compiler, not documentation, was the source of truth for the exact 3.1.0
surface. Captured output:

```
     Running tests/stdio_roundtrip.rs (target/debug/deps/stdio_roundtrip-...)
running 1 test
test echo_tool_round_trips_over_stdio ... ok

     Running tests/streamhttp_roundtrip.rs (target/debug/deps/streamhttp_roundtrip-...)
running 1 test
test echo_tool_round_trips_over_streamable_http ... ok
```

A manual `curl` against the running `echo_streamhttp` binary confirms the
same server also speaks plain JSON-RPC over the wire, independent of the
rmcp client SDK:

```
$ curl -sS -X POST http://127.0.0.1:8765/mcp \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"manual-curl","version":"0.0.1"}}}'

data: {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2026-07-28","capabilities":{"tools":{}},"serverInfo":{"name":"rmcp","version":"3.1.0"}}}
```

Two secondary observations from that check: the default `server_handler`
macro reports `serverInfo.name` as the generic `"rmcp"`, not a Totem-specific
name — `ADV-GATEWAY-002` should set this explicitly via `#[tool_handler(name
= ..., version = ...)]` (README §Tools) rather than the macro default. And
the response arrived `text/event-stream`-framed (`data: {...}`) even though
`StreamableHttpServerConfig::default()` was used — per the README's
"Stateless Streamable HTTP" section, plain-JSON replies need
`with_json_response(true)` explicitly; the default is SSE framing even for
simple request/response tools.

### MCP-002 — `rmcp` is a mature, first-party, actively-maintained crate already built on Totem's own HTTP framework. *(confirmed, from the crates.io index)*

Queried `https://index.crates.io/rm/cp/rmcp` (the sparse index; `crates.io`'s
own web/API host returned `403` in this sandbox, but the index host that
`cargo` itself uses did not — see MCP-004's environment note):

- Published by the `modelcontextprotocol` GitHub org — the same org that
  publishes the MCP specification itself, not a third-party reimplementation.
- 58 published versions, `0.1.0` (2025-03-16) → `1.0.0` (2026-03-03) →
  `2.0.0` (2026-06-29) → `3.0.0` (2026-07-28) → `3.1.0` (2026-07-31) — the
  latest release landed 5 days before this investigation.
- The `server` feature's `server-side-http` dependency requires `axum ^0.8`
  — the same major version Solution Intent §1/§7 commits `totem-gateway` to.
  `StreamableHttpService` is a Tower service mountable directly on an
  `axum::Router` (confirmed by execution in MCP-001), not a separate HTTP
  stack to bridge.
- Relevant feature flags for `totem-gateway`: `transport-io` (stdio),
  `transport-streamable-http-server` (cloud), `auth` /
  `auth-client-credentials-jwt` (OAuth2 + JWT, relevant to ADV-GATEWAY-003's
  least-privilege cloud tokens — not exercised by this spike, noted for that
  advance to evaluate against its own auth design rather than rmcp's).

The version cadence — three breaking major versions in five months after a
year to reach `1.0`, one landing days before this run — is itself evidence
for the brief's named "standard drift" risk: this pin will need re-checking
at ADV-GATEWAY-002 time, not treated as settled.

### MCP-003 — Claude Code (desktop CLI and web/cloud sessions) supports remote MCP over streamable HTTP as its recommended transport, with OAuth 2.0. *(confirmed, primary docs + self-observed for web sessions)*

Fetched `https://code.claude.com/docs/en/mcp` directly (reachable from this
sandbox, unlike `crates.io`'s web host):

- Four transports: `stdio` (local child process), `http` (streamable HTTP,
  `--transport http`, **"the recommended option for connecting to remote MCP
  servers... the most widely supported transport for cloud-based
  services"**), `sse` (deprecated, kept for legacy servers), `ws`
  (WebSocket, for servers that push unprompted events — no OAuth support,
  header-only auth).
- `type: "streamable-http"` is accepted as an alias for `"http"` in JSON
  config, so server-authored config copy-pastes without translation.
- OAuth 2.0 is a first-class flow: automatic discovery via `401`/`403` +
  `WWW-Authenticate`, Dynamic Client Registration, or Client ID Metadata
  Document fallback, plus a fixed-callback-port option for servers that
  require pre-registered redirect URIs.
- Explicitly documents behavior inside **web sessions** (`/docs/en/claude-code-on-the-web`):
  an MCP call to a plugin server that isn't connected yet — e.g. right after
  an idle web session wakes — starts the server on demand and waits for the
  connection.

**Self-observed, not just documented:** the session that ran this
investigation is itself a Claude Code web/cloud session with two live MCP
server connections at the time of writing (`Claude_Code_Remote`, `github`,
both listed in this session's own tool namespace). That is direct,
real-time evidence that this harness class can and does attach remote MCP
today — not a claim sourced from documentation alone.

### MCP-004 — The Claude API's MCP connector (the mechanism generic "Anthropic cloud agent" integrations use) reaches servers only over Streamable HTTP or SSE at a public HTTPS URL, tools-only. *(confirmed, primary docs)*

Fetched `https://platform.claude.com/docs/en/agents-and-tools/mcp-connector`
directly:

- **"The server must be publicly exposed through HTTP (supports both
  Streamable HTTP and SSE transports). Local STDIO servers cannot be
  connected directly."** A cloud-hosted Totem gateway is a fit; a
  desktop-only stdio deployment is not reachable from this path at all.
- **Tools only** — of the full MCP feature set, only tool calls are
  supported (no resources, prompts, sampling, or roots) via this connector.
  This matters for `ADV-GATEWAY-002`'s tool surface design (`totem_recall`,
  `totem_save`, etc. per Solution Intent §3.1 are tool calls, so this is not
  a blocker) but rules out leaning on MCP resources/prompts for this
  specific integration path.
- Auth: `authorization_token` (OAuth Bearer) per server definition; the
  caller (not the connector) is responsible for obtaining and refreshing it.
- Current beta header `mcp-client-2025-11-20`; the prior
  `mcp-client-2025-04-04` is deprecated — another concrete instance of the
  "standard drift" risk, this time on the client side of the protocol.

Network note on how this was obtained: `curl` directly to
`docs.anthropic.com` returned `403` and `modelcontextprotocol.io` failed the
proxy `CONNECT` tunnel entirely, but the `WebFetch` tool reached
`platform.claude.com` and `code.claude.com` successfully — this sandbox's
tool-level fetch path and its raw-`curl`-through-`HTTPS_PROXY` path do not
have identical reachability. `index.crates.io` and `raw.githubusercontent.com`
were reachable directly via `curl`; `crates.io`'s own web/API host and
`api.github.com` (for this specific repo) were not.

### MCP-005 — Cursor's remote and background-agent MCP reach could not be independently verified in this sandbox. *(not executed — environment-blocked, secondary sources only)*

`docs.cursor.com` failed with `CONNECT tunnel failed, response 403` via both
`curl` and the `WebFetch` tool — the same class of egress block
ADV-STORE-003/004/006 documented for other hosts. What follows is sourced
from `WebSearch` result summaries of third-party 2026 setup guides (not
Cursor's own documentation, and not independently verified against it):

- Cursor is reported to support both `stdio` (local child process) and
  streamable HTTP for remote servers, with streamable HTTP described as "the
  recommended choice" for new remote servers.
- Cursor's background agents are reported to launch MCP servers "as a child
  process or connect to it over HTTP for remote servers" — the same
  transport story as the local IDE agent, per these secondary sources, but
  this was not confirmed against a primary source or an actual Cursor
  session.

This is a real gap, not a rounding error: Solution Intent §9 names Cursor
background agents explicitly as a "cloud harness" Totem must reach, and
ADV-GATEWAY-003's least-privilege token design should not assume Cursor's
capabilities are identical to Claude Code's without confirming this from
Cursor's own docs or a live session first.

## 3. Per-harness capability matrix

| Harness | Transports | Auth | Verified how |
|---|---|---|---|
| Claude Code (desktop CLI) | stdio, HTTP/streamable-HTTP (recommended), SSE (deprecated), WebSocket | OAuth 2.0 (DCR + CIMD fallback), static headers | **Documented** — primary source (`code.claude.com/docs/en/mcp`), fetched live |
| Claude Code (web/cloud sessions) | Same as desktop CLI; servers provisioned by the remote host | Same as desktop CLI | **Documented + self-observed** — this investigation ran inside such a session with live MCP connections at time of writing |
| Anthropic cloud agents via the Claude API MCP connector | Streamable HTTP or SSE only, server must be public HTTPS; no stdio | OAuth Bearer token (`authorization_token`), caller-managed | **Documented** — primary source (`platform.claude.com/docs/en/agents-and-tools/mcp-connector`), fetched live |
| Cursor (local IDE agent) | stdio, streamable HTTP (per secondary sources) | Not confirmed | **Unverified** — `docs.cursor.com` blocked in this sandbox; secondary sources only |
| Cursor (background agents) | Reported: child process or HTTP, matching local agent | Not confirmed | **Unverified** — same block; secondary sources only |

## 4. Recommendation

**Adopt `rmcp`, pinned `=3.1.0`.** It is the official SDK, it is what
Solution Intent §7 already names, its streamable-HTTP server mounts directly
on Totem's own `axum` gateway with no bridging layer, and both transports
Totem needs were exercised end-to-end here, not just read about. No
alternative (hand-rolled protocol layer, another SDK) was evaluated because
none was needed — `rmcp` cleared the bar this spike set for it.

**Design `totem-gateway`'s MCP surface (`ADV-GATEWAY-002`) against streamable
HTTP as the primary remote transport**, matching what both verified harnesses
(Claude Code, the Claude API connector) actually prefer or require — stdio
stays the desktop-local path. Do not assume feature parity with resources,
prompts, or sampling for the Claude-API-connector integration path
specifically (MCP-004); tool calls only.

**Carry MCP-005's gap into `ADV-GATEWAY-003` explicitly.** Its least-privilege
cloud-token design (Solution Intent §3.3) should either re-run this
capability check from a Cursor session or an environment that can reach
`docs.cursor.com`, or explicitly accept Cursor's capabilities as an
unverified assumption rather than a confirmed one.

**Re-verify the rmcp pin before `ADV-GATEWAY-002` starts implementation.**
MCP-002's version cadence (three major versions in five months, the latest
five days before this run) means "current state at implementation time"
(Solution Intent §7) is a standing instruction, not a one-time check this
advance discharges permanently.

## 5. What this spike deliberately did not answer

- Auth flows (OAuth, JWT, the least-privilege token design) — `rmcp`'s `auth`
  feature exists (MCP-002) but was not exercised; that is
  `ADV-GATEWAY-003`'s job, against its own scope-bound token design, not
  rmcp's generic OAuth helper.
- Multi-tool, stateful servers, resources, prompts, or sampling — `Echo` is
  intentionally the smallest possible tool surface; `ADV-GATEWAY-002` designs
  the real `totem_recall`/`totem_save`/`totem_landscape` tool set.
- Session/reconnection behavior under real network conditions (the stateless
  default this spike used has no `Mcp-Session-Id`; `ADV-GATEWAY-003` should
  decide statelessness vs. session-scoped state deliberately, not inherit the
  spike's default by accident).
- Live attachment testing from an actual Cursor session, or an actual Claude
  Desktop app — this sandbox has neither; MCP-001/MCP-003's client-side
  evidence comes from rmcp's own client SDK and this session's own live
  connections, not from operating those specific other applications.

## Evidence

- `cargo test -p totem-mcp-spike` — 2/2 pass, real client↔server round trips
  over both transports, no mocking (MCP-001).
- Manual `curl` against the running `echo_streamhttp` binary — raw JSON-RPC
  `initialize` response captured above (MCP-001).
- `https://index.crates.io/rm/cp/rmcp` (sparse index, fetched live) — version
  history and feature-flag data (MCP-002).
- `https://code.claude.com/docs/en/mcp` (fetched live via `WebFetch`) —
  transport and auth reference (MCP-003).
- This session's own active MCP connections, observed directly, not sourced
  from documentation (MCP-003).
- `https://platform.claude.com/docs/en/agents-and-tools/mcp-connector`
  (fetched live via `WebFetch`) — connector transport/auth/limitations
  reference (MCP-004).
- `WebSearch` results summarizing third-party Cursor MCP setup guides, dated
  2026 — explicitly labeled as unverified secondary sources, not
  independently confirmed (MCP-005).
