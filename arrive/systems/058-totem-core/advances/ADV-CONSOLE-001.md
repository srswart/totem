---
advance:
  id: "ADV-CONSOLE-001"
  title: "Landscape dashboard + memory browser (read-only)"
  system: "058-totem-core"
  primary_component: "console"
  components: ["console", "gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T07:18:57Z"
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 88
  risk_flags: ["new_dependency", "concurrency"]
  evidence: ["tests:unit", "tdd:red-green"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: in_progress
---

## Objective

First Dioxus console (web target): read-only landscape dashboard
(systems/components/advances across enrolled repos) and memory browser filtered
by scope and category (Solution Intent §5). Console updates ride SurrealDB
live queries through the gateway.

## Behavioral Change

After this advance:
- A developer can open the console, enter a repo id, actor, and project, and
  see that repo's landscape (systems/components/advances) and their own
  readable memories grouped by category — the first "humans observe" surface
  (G5).
- **Correction to this advance's original claim:** landscape/memory updates
  do **not** appear without a manual refresh. The advance as written assumed
  live-query auto-update; what actually shipped is a "Refresh" button. TD-009
  is correctly cited (live queries only exist on embedded/WebSocket SurrealDB
  connections, so the console must consume gateway *events*, not open its own
  DB connection) but building that event relay (gateway subscribing to a live
  query and forwarding notifications to the browser — SSE or WebSocket) is a
  distinct subsystem this advance did not build. Recorded honestly rather than
  narrowed silently; see Risk + Rollback.
- `GET /landscape/:repo` is a new REST route (`totem-gateway`), added because
  the only prior landscape read was the MCP tool `totem_landscape`
  (ADV-GATEWAY-002), unusable from a browser. It calls the same
  `totem_store::LandscapeRepository::view` the MCP tool does, so the two
  surfaces cannot diverge on what a repo's landscape contains.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change) — none needed; see
      Check for Understanding for why.
- [x] test: component/view tests where the Dioxus toolchain supports them;
      API-contract tests otherwise
- [x] feat: Dioxus app shell, landscape dashboard, memory browser

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: Dioxus web toolchain maturity. Mitigated as planned — views are thin
  over the REST API (`view_model.rs` parses, `app.rs` only renders) — but the
  toolchain itself is more absent than immature in this sandbox: no `dx` CLI,
  no `trunk`, no browser. The wasm32-unknown-unknown build was verified with
  `cargo check`/`cargo clippy --target wasm32-unknown-unknown`, which proves
  it compiles and links, but the app has never actually been run in a
  browser. Treat "does it render correctly under `dx serve`" as unverified
  until someone with that tooling checks it out.
- Risk (new, not in the original advance): **no live-query relay.** The
  Behavioral Change's auto-update claim does not hold — see the correction
  above. Follow-up work (not scoped here) needs a gateway-side subscription
  to the landscape's live query, forwarded to the browser over SSE or
  WebSocket, and a console-side listener that patches `landscape`/`memories`
  signals instead of only responding to the Refresh button.
- Risk (new): **no console authentication.** `RootApp`'s actor/project fields
  are free-text input, matching the open question in
  docs/arrive-decomposition-gaps.md ("Console human authentication ...
  likely reverse-proxy/SSO initially") — anyone who can reach the console can
  read any actor's memories by typing their id. Acceptable for a
  single-team, reverse-proxied deployment (Solution Intent §9's assumed
  topology) but not safe to expose more broadly before ADV-GATEWAY-003
  (token auth) and a real console login exist.
- Rollback: revert branch; console is read-only, no data at risk. The new
  `GET /landscape/:repo` route is additive and used nowhere else yet, so
  reverting it has no other callers to break.

## Reviewability

`arrive score --base origin/advance/phase-005` (this sub-PR's actual base)
reports **88 [RED]** — Size 63, Novelty 20, Risk 5 (flag: `concurrency`).
Submitted as one sub-PR anyway rather than split further:

- The hand-authored diff is 965 lines across 12 files (`git diff --stat
  origin/advance/phase-005..HEAD -- . ':(exclude)Cargo.lock'`). The rest of
  the size is `Cargo.lock` (+612/-24 lines): mechanical, tool-generated
  entries for Dioxus's transitive dependencies, not something a reviewer
  reads line by line — `cargo tree -p totem-console` is the right way to
  review that layer, not the lockfile diff.
- The four commits are each independently reviewable and build in order:
  the gateway endpoint (test, then feat), the crate scaffold, the
  target-agnostic view models + components, then the wasm-only wiring. A
  reviewer can stop after any commit and understand what it adds.
- Splitting into multiple sub-PRs was considered and rejected: an "empty
  console crate scaffold" sub-PR has no behavior to review, and a console
  crate without `view_model`/`app` doesn't compile — the natural split
  points are the four commits already in this one sub-PR, not sub-PR
  boundaries.
- The `concurrency` flag comes from `api.rs`'s `spawn(async move { ... })`
  calls (Dioxus's async task spawning for the Refresh button's fetch calls)
  — ordinary async UI wiring, not shared mutable state or a race condition;
  there is no lock, no shared counter, nothing two tasks contend over.

## Evidence

- [x] tdd:red-green — `crates/totem-gateway/tests/landscape.rs`: written and
      confirmed failing (404, no route) before
      `crates/totem-gateway/src/handlers.rs::landscape` existed; green after.
- [x] tests:unit — `crates/totem-console/src/view_model.rs` (6 tests) and
      `crates/totem-console/src/app.rs` (5 `dioxus-ssr` render tests, 11
      total). **Not** tdd:red-green: these were authored together with their
      implementation in the same file/commit rather than as an observed
      red-then-green cycle, and are recorded as `tests:unit` only, per the
      honesty rule in docs/cloud-agent-notes.md ("Only claim tdd:red-green if
      you genuinely wrote failing tests first").
- [ ] tests:e2e — not attempted: no browser or `dx`/`trunk` in this sandbox.
      `cargo check --target wasm32-unknown-unknown -p totem-console` and
      `cargo clippy --target wasm32-unknown-unknown -p totem-console
      --all-targets -- -D warnings` are the closest verification reachable
      here (both clean) — compiles and links for the web target, never
      rendered in one.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CONSOLE-001 --status passed`

## Changes Made

### 2026-08-06 - test: GET /landscape/:repo returns the landscape over REST
- crates/totem-gateway/tests/landscape.rs: new — unsynced repo returns an
  empty landscape (not an error); a synced repo's systems/components/
  advances round-trip over REST
- crates/totem-gateway/tests/common/mod.rs: added a `get` helper (only `post`
  existed)

### 2026-08-06 - feat: GET /landscape/:repo REST endpoint
- crates/totem-gateway/src/handlers.rs: new `landscape` handler, calling the
  same `totem_store::LandscapeRepository::view` `mcp.rs`'s `totem_landscape`
  tool uses
- crates/totem-gateway/src/dto.rs: re-export `totem_store::LandscapeView` as
  the REST response body
- crates/totem-gateway/src/lib.rs: route `GET /landscape/{repo}`, re-export
  `LandscapeView`

### 2026-08-06 - feat: totem-console crate scaffold
- Cargo.toml: register `crates/totem-console`; add uuid's `"js"` feature
  workspace-wide (needed once totem-core, via totem-console, targets
  wasm32-unknown-unknown)
- crates/totem-console/Cargo.toml: new manifest — dioxus core features as a
  regular dependency, `dioxus-web`/`gloo-net` as wasm32-only target
  dependencies, `dioxus-ssr` as a dev-dependency

### 2026-08-06 - feat: view models + Dioxus app shell (landscape + memory browser)
- crates/totem-console/src/view_model.rs: `LandscapeViewModel` and its parts
  (mirroring `totem_store::landscape`'s wire shape with `Deserialize`,
  without depending on `totem-store` itself), `parse_landscape`,
  `parse_memories` (reusing `totem_core::MemoryRecord` directly),
  `group_by_category`
- crates/totem-console/src/app.rs: `App` (tab shell), `LandscapeView`,
  `MemoryBrowserView`, `ComponentRow`, `AdvanceRow`, `CategoryGroup`
- crates/totem-console/src/lib.rs: wire the two modules in

### 2026-08-06 - feat: wasm entry point + gateway HTTP client
- crates/totem-console/src/api.rs: `fetch_landscape`, `fetch_memories`
  (`gloo-net`), `RootApp` (repo/actor/project form + manual Refresh over
  `App`)
- crates/totem-console/src/main.rs: `wasm32`-gated `dioxus_web::launch`;
  native build is a stub printing how to build for the real target
- crates/totem-console/src/lib.rs: wire the `api` module in (wasm32-only)

### 2026-08-06 - fix: address Copilot review on PR #22 (api.rs input hygiene)
- crates/totem-console/src/api.rs: `fetch_landscape` now trims and
  percent-encodes `repo` before it reaches the fetch URL (an untrimmed or
  `/`-containing value previously changed which route the browser actually
  requested); `fetch_memories` trims `actor`/`project` and requests a
  bounded `RECALL_LIMIT` (200) instead of an unbounded `limit: null`;
  `RootApp`'s refresh closure clears `error` at the start of every refresh
  and clears `memories` when the actor/project fields are blank, instead of
  leaving stale error/result state on screen
- crates/totem-console/Cargo.toml: `percent-encoding` as a wasm32-only
  target dependency

## Check for Understanding

1. `GET /landscape/:repo` and the MCP tool `totem_landscape` both end up
   calling `totem_store::LandscapeRepository::view`. Why does that matter for
   an unsynced repo's response, and where in the code is that guaranteed
   rather than merely likely?
2. `totem-console`'s `Cargo.toml` puts `dioxus-web` and `gloo-net` under
   `[target.'cfg(target_arch = "wasm32")'.dependencies]` instead of ordinary
   `[dependencies]` gated with `#[cfg(...)]` in code. What class of failure
   does that avoid when `cargo clippy --workspace --all-targets` runs on the
   native host, and why would a code-level `#[cfg]` alone not avoid it?
3. `ViewModelError::Json`'s failure field is named `detail`, not `source`.
   What would have gone wrong with `#[derive(thiserror::Error)]` if it had
   been named `source`, given the field's type is `String`?
4. The Behavioral Change section describes live-query auto-update as
   planned but not delivered. Trace why: which tech-direction constraint
   (see docs/tech-direction/surrealdb.md) makes this a real subsystem rather
   than a small addition, and what would the gateway side of that subsystem
   need to do?
5. `view_model.rs`'s tests construct `MemoryRecord` via `MemoryRecord::new`
   and round-trip it through `serde_json`, rather than hand-writing a JSON
   literal the way `landscape_json()` does for the landscape shape. Why is
   that the safer choice for `MemoryRecord` specifically?
