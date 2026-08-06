---
advance:
  id: "ADV-GATEWAY-002"
  title: "MCP server (stdio) exposing recall/save/landscape"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T04:28:09Z"
  review_time_estimate_minutes: 126
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 70
  risk_flags: ["public_api", "new_dependency", "concurrency"]
  evidence: ["tidy:preparatory", "tdd:red-green", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

Serve MCP over stdio for desktop harnesses (Claude Code, Cursor) with the
initial tool surface: `totem_recall`, `totem_save`, `totem_landscape`
(Solution Intent §3.1). Verify the current state of the Rust MCP SDK (`rmcp`)
at implementation time, per §7.

## Behavioral Change

After this advance:
- A desktop harness configured with the Totem MCP server (`totem-gateway`'s
  `mcp_stdio` binary) can call `totem_recall` and `totem_save` and get the
  same behavior `POST /recall`/`POST /save` give over REST — merged,
  scope-resolved context on read; provenance auto-attached on write — because
  both surfaces now call the same `ops::recall`/`ops::save` functions.
- Every MCP tool call that reaches the store and completes appends one access
  log entry, labeled `mcp:totem_recall` / `mcp:totem_save` (vs. REST's
  `/recall` / `/save`), so an audit query can tell which surface a caller
  used without losing "every read and write is logged" (G3).
- `totem_landscape` is callable and returns valid JSON, but — see "Partially
  established invariant" below — it does not yet return real landscape data:
  `ADV-ARRIVE-SYNC-001` (the advance that ingests `/arrive/` into a landscape
  graph) has not landed, so there is nothing real to query yet.
- A caller writing outside its own resolved scope chain gets a protocol-level
  MCP error (`invalid_params`), not a silently-empty response — mirroring the
  REST surface's `403 Forbidden`, in MCP's own idiom (JSON-RPC error, not an
  HTTP status).
- The `rmcp` pin (`=3.1.0`) was re-verified at implementation time per
  Solution Intent §7's standing instruction (docs/tech-direction/mcp.md's
  recommendation): the same version ADV-GATEWAY-005 spiked still builds and
  its `tool_router(server_handler)` + stdio-transport pattern from that spike
  carries over directly, no API drift encountered.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: extract `ops::save`/`ops::recall` out of `handlers.rs` so REST and
      MCP call the exact same operations (no behavior change; the pre-existing
      8 REST integration tests pass unchanged)
- [x] test: tool-dispatch tests over the MCP layer (red first) —
      `tests/mcp_recall_and_save.rs`, a real `rmcp` client over a real stdio
      child-process transport
- [x] feat: stdio MCP server (`TotemMcp` in `mcp.rs`, served by the new
      `mcp_stdio` binary) with `totem_recall`/`totem_save`/`totem_landscape`
      tools

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: `rmcp` maturity / MCP spec drift ("standard drift" risk); mitigated by
  keeping the MCP layer a thin adapter over `ops` — `mcp.rs` contains parameter
  parsing and JSON encoding, no business logic of its own.
- Risk (realized, documented rather than avoided — see "Partially established
  invariant" below): `totem_landscape` has no real landscape data behind it
  yet. Exposing the tool now, honestly empty, was chosen over deferring the
  whole surface until `ADV-ARRIVE-SYNC-001` lands, so a harness configuring
  Totem's MCP server today sees the full advertised tool surface rather than
  two tools out of three.
- Risk (new in this advance): MCP tool parameters are plain JSON primitives
  (`String`, `serde_json::Value`), not `totem-core`'s own validated types
  directly — `schemars::JsonSchema` (needed for MCP's tool input schema)
  isn't implemented on `totem-core`'s hand-validated newtypes, and adding it
  there would be a `core`-component change outside this advance's declared
  scope (`gateway` only). Every field is still parsed through `totem-core`'s
  own fallible constructors before reaching `ops` (`mcp.rs`'s own doc
  comment), so validation itself is not duplicated — only the envelope shape
  is. A future advance that wants a stricter, self-describing MCP input
  schema would add `schemars` support to `totem-core` behind a feature flag.
- Rollback: revert branch; REST surface (`handlers.rs`, `router`) is
  unaffected — `ops.rs`'s extraction changed no REST behavior, confirmed by
  the pre-existing REST suite passing unchanged both before and after.

## Evidence

- [x] tidy:preparatory — `ops.rs` extracts `save`/`recall` out of
      `handlers.rs`; the pre-existing `tests/recall_and_save.rs` (8 tests)
      passed unchanged before and after, confirming no REST behavior change
- [x] tdd:red-green — confirmed by execution. `tests/mcp_recall_and_save.rs`
      was written and committed (`test:` commit) before the `rmcp` production
      dependency, the `mcp` module, or the `mcp_stdio` binary existed;
      `cargo test -p totem-gateway --test mcp_recall_and_save` failed to
      compile at that point (`unresolved crate rmcp` in `mcp.rs`, which was
      itself unreferenced dead code until the `feat:` commit wired it in).
      After the `feat:` commit, all 5 tests pass.
- [x] tests:integration — `tests/mcp_recall_and_save.rs` (5 tests, real
      `rmcp` client over a real stdio child-process transport spawning the
      compiled `mcp_stdio` binary, no mocking): tool-surface advertisement,
      save→recall round trip, cross-actor scope isolation, a denied write,
      and `totem_landscape`'s honest-empty response. The pre-existing
      `tests/recall_and_save.rs` (8 REST tests) continues to pass, proving
      `ops.rs`'s extraction was behavior-preserving.
- [ ] tests:unit — **not claimed.** No new unit tests were added by this
      advance; `ops.rs`, `mcp.rs`'s parsing helpers, and the tool dispatch are
      covered only by the integration suites above. A future advance wanting
      finer-grained coverage of `mcp.rs`'s parameter-parsing edge cases (a
      malformed `harness` JSON value, an empty `actor` string) would add it
      there.
- [ ] ci:passed — **not claimed.** CI runs on `advance/**` pushes; this
      record was written before the pipeline result for this branch was
      available.

Workspace verification at the time of writing: `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace` all clean; `arrive doctor artifacts` 0 issues;
`arrive plan check` valid; `arrive check --strict` clean.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-GATEWAY-002 --status passed`

## Reviewability

`arrive score --base origin/advance/phase-004` reports **70 [RED]** (size 52,
novelty 13, risk 5: `concurrency` — a pattern match on `Cargo.lock`'s new
`tokio`/`process`-feature entries rather than any concurrency-relevant
application logic this advance adds). Documented rather than split, for
essentially ADV-GATEWAY-001's own reasoning:

- Roughly two-thirds of the diff by file count is either mechanical
  extraction (`ops.rs` moving existing logic verbatim out of `handlers.rs`,
  164 lines) or dependency/manifest bookkeeping (`Cargo.toml`, `Cargo.lock`).
  The genuinely new logic is `mcp.rs` (285 lines: three tool methods, their
  parameter structs, and the parsing glue that converts JSON primitives into
  `totem-core` types) and the binary (27 lines).
- Splitting `ops.rs`'s extraction into its own advance is not possible under
  this repo's process — a `tidy:` commit is preparatory refactoring *for* a
  behavioral change, not an independent unit of review; ARRIVE's own
  dev-practices rules require it to precede the `test`/`feat` commits it
  prepares for, inside the same advance.
- Splitting `totem_recall`/`totem_save` from `totem_landscape` into separate
  sub-PRs would ship two-thirds of "the initial tool surface" the objective
  names as one deliverable (Solution Intent §3.1), and would prevent
  `the_full_tool_surface_is_advertised` from existing as a single test that
  proves the whole surface, not two independently-shipped fragments of it.
- Splitting the test commit from the feat commit is already done (separate
  commits in this advance) but they cannot be separate *sub-PRs*: a red test
  with no implementation behind it is not itself a reviewable, working state
  to merge into the phase branch.

What was *not* pulled into this advance to keep it from being larger still:
streamable HTTP transport and auth (ADV-GATEWAY-003, per the tech direction's
explicit recommendation to design that surface against its own token model
separately), real landscape data (`ADV-ARRIVE-SYNC-001`), and `schemars`
support on `totem-core`'s own types (noted above, under Risk).

## Changes Made

### 2026-08-06 - tidy: extract ops.rs shared save/recall operations
- `crates/totem-gateway/src/ops.rs`: new — `SaveInput`/`RecallInput` plus
  `save`/`recall` functions that resolve the scope chain, do the store
  operation, and append one access-log entry, parameterized by an `endpoint`
  label so REST and MCP can pass different values (`/save` vs.
  `mcp:totem_save`) into the same audit trail.
- `crates/totem-gateway/src/handlers.rs`: rewritten to build an `ops::*Input`
  directly from the request's own `totem-core` types and call the shared
  operation, instead of inlining the resolve/store/log sequence itself. No
  behavior change: the pre-existing 8 REST integration tests pass unchanged.
- `crates/totem-gateway/src/lib.rs`: registers the new `ops` module.

### 2026-08-06 - test: MCP tool-dispatch tests over stdio (red first)
- `crates/totem-gateway/tests/mcp_recall_and_save.rs`: new — 5 tests driving
  a real `rmcp` client over a real stdio child-process transport against the
  not-yet-implemented `mcp_stdio` binary. Confirmed to fail to compile before
  the `feat:` commit (no `rmcp` production dependency, no `mcp` module, no
  `mcp_stdio` binary target existed yet).
- `crates/totem-gateway/Cargo.toml`: adds `rmcp` (`client`,
  `transport-child-process` features) and `tokio` (`process` feature) as
  dev-dependencies for the test harness only.

### 2026-08-06 - feat: stdio MCP server (totem_recall/totem_save/totem_landscape)
- `crates/totem-gateway/src/mcp.rs`: new — `TotemMcp`, an
  `rmcp::tool_router(server_handler)` server exposing `totem_recall`,
  `totem_save`, and `totem_landscape`. Parameter structs
  (`RecallParams`/`SaveParams`/`LandscapeParams`) use plain JSON primitives
  (`schemars::JsonSchema` for the tool input schema); each tool parses its
  params through `totem-core`'s own fallible constructors before calling
  `ops::recall`/`ops::save`, and maps `GatewayError` to an MCP `ErrorData` the
  same way `error.rs` maps it to an HTTP status (a caller-actionable rule vs.
  an opaque internal error).
- `crates/totem-gateway/src/bin/mcp_stdio.rs`: new — the binary that builds
  an embedded, in-memory store (same topology as `main.rs`'s REST binary) and
  serves `TotemMcp` over stdio.
- `crates/totem-gateway/src/lib.rs`: registers the `mcp` module, exports
  `TotemMcp`, and updates the crate-level doc comment to describe both
  surfaces.
- `crates/totem-gateway/Cargo.toml`: adds `rmcp` (`transport-io` feature,
  pinned `=3.1.0` matching ADV-GATEWAY-005's spike) and `anyhow` to
  `[dependencies]`, and a `mcp_stdio` `[[bin]]` target.
- `Cargo.lock`: updated for the new dependency graph.

## Check for Understanding

1. `ops::save` and `ops::recall` (`crates/totem-gateway/src/ops.rs`) take an
   `endpoint: &str` parameter that both `handlers.rs` and `mcp.rs` pass
   differently (`"/save"`/`"/recall"` vs. `"mcp:totem_save"`/`"mcp:totem_recall"`).
   What does this parameter get used for downstream, and how would you query
   the access log to see only MCP-originated writes?
2. `mcp.rs`'s tool parameter structs (`RecallParams`, `SaveParams`) type
   `harness` and `author` as `serde_json::Value` rather than
   `totem_core::Harness`/`Author` directly. Why couldn't they use the
   `totem-core` types directly, and where does the actual validation of those
   fields happen instead?
3. `writing_into_another_actors_scope_over_mcp_is_refused`
   (`tests/mcp_recall_and_save.rs`) asserts `result.is_err()` on the
   `call_tool` future itself, not on a successful result with an error flag
   set. Trace through `gateway_error` and rmcp's `IntoCallToolResult` impls
   to explain why a scope-denied `totem_save` surfaces as a protocol-level
   error rather than a tool result with `is_error: true`.
4. `totem_landscape` always returns `{"systems": [], "components": [],
   "advances": [], "note": "..."}`. What advance would make this return real
   data, and what would have to exist in `totem-store` first?
5. This advance's `arrive score` came back 70 [RED] and was documented rather
   than split (see "Reviewability" above). Which two files account for most
   of the "new logic" share of the diff, and which files are the mechanical
   or bookkeeping share?
