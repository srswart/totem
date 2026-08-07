---
advance:
  id: "ADV-CONSOLE-003"
  title: "Live landscape updates: gateway event relay + console subscription"
  system: "058-totem-core"
  primary_component: "console"
  components: ["console", "gateway", "store"]
  started_at: "2026-08-06T09:05:00Z"
  implementation_completed_at: "2026-08-07T06:45:00Z"
  review_time_estimate_minutes: 127
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 69
  risk_flags: ["concurrency"]
  evidence: ["tidy:preparatory", "tdd:red-green", "tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
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
- `totem-gateway` exposes `GET /landscape/:repo/events`, an SSE stream that
  sends the current `LandscapeView` immediately on connect and again on every
  change to the landscape's own tables (`repo`/`system`/`component`/
  `advance`), via `totem-store`'s new `LandscapeRepository::watch()`.
- The console (`RootApp`) subscribes to that stream and patches its
  `landscape` signal in place; the Refresh button remains as a manual
  fallback and is still the update path for memories/promotions/uncertainty.
- The relay endpoint authorizes exactly like `GET /landscape/:repo` (the same
  `authorize_repo` check against the bound `git_repo`), and every event it
  emits — the initial one and every pulse-triggered one — is a fresh,
  store-enforced `landscape().view()` read, logged with its own
  `AccessOperation::Recall` entry (no unlogged access path).

### Scope correction (recorded honestly, not silently narrowed)

- **Memory-table live relay is deferred, not delivered.** The Behavioral
  Change originally planned here also covered the memory browser's
  `POST /recall` polling. That is out of scope for this advance: a
  scope-correct memory relay needs to filter *per subscriber* through a
  resolved `ScopeChain` (the store-enforced predicate `MemoryRepository::recall`
  already applies to a one-shot read), which is materially more work than the
  landscape relay — landscape has no per-caller scope filter at all, only the
  repo binding `authorize_repo` already checks. Bundling both into one
  advance would have meant either shipping the memory relay hastily (a
  scope-leak-adjacent surface, per this repo's own security-critical-advances
  guidance) or blocking the landscape relay on it. A follow-up advance should
  scope the memory relay on its own.
- **The live-query trigger is deliberately unfiltered by repo at the
  SurrealDB layer**, merging one `LIVE SELECT` per landscape table
  (`repo`/`system`/`component`/`advance`) into a payload-free pulse
  (`LandscapeChanges`). SurrealDB's live-query `WHERE` clause filters the
  mutated row's own fields, not a dereferenced link two hops away
  (`component.system.repo`, `advance.system.repo`), so a repo-scoped
  server-side filter is not reliably expressible at that layer today. This is
  safe specifically because the pulse carries no data — every subscriber
  re-reads through the store-enforced, repo-scoped `view()` before anything
  reaches a browser — but it means every enrolled repo's changes wake every
  subscriber's relay to re-check, which does not scale past a handful of
  concurrently-watched repos. Acceptable for a first cut; a future advance
  should either add a schema-level `repo` field to `component`/`advance` rows
  (flattening the traversal) or move the fan-out to a per-repo channel.
- **Browser verification was not performed and cannot be performed in this
  sandbox** (no browser, no live gateway process to point one at) — the same
  category of residual ADV-CONSOLE-001 already recorded for `dx serve`
  verification. The gateway relay is verified end-to-end by real HTTP tests
  driving the composed `Router`; the console's wasm-only `EventSource`
  wiring (`api.rs`) compiles clean for `wasm32-unknown-unknown` but its
  actual browser behavior — does the subscription reconnect correctly, does
  `use_future`'s cancellation actually tear down the old `EventSource` on a
  repo change — is unverified. A workstation session should confirm this
  with `dx serve` against a running gateway before this surface is trusted
  for anything beyond a demo.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: extracted `handlers::landscape_git_repo` so `GET /landscape/:repo`
      and the new relay share one git_repo resolution
- [x] test: gateway relay tests against the embedded in-memory engine with a
      real HTTP client driving the composed `Router` (`tests/landscape_events.rs`),
      reading the SSE body incrementally frame-by-frame; console-side unit
      test proving a relayed event's payload parses identically to a polled
      fetch body (`view_model::parse_landscape_event`)
- [x] feat: implement the minimal changes to pass tests — see Changes Made

## Bug Fixes

- Fixed during this advance's own TDD cycle (not a defect in already-merged
  code): `tests/landscape_events.rs`'s second-event assertion originally
  checked `advances[0].id`, which broke because `sync` never deletes an
  advance absent from a later snapshot — enrolling a second advance id grows
  the set rather than replacing it, so array position is not guaranteed.
  Fixed to assert membership instead (commit `8d9f405`).

## Risk + Rollback

- Risk: long-lived streaming connections are the gateway's first stateful
  client surface. Mitigated by construction rather than by explicit
  lifecycle code: the whole relay lives inside the response body's own
  `async_stream::stream!` generator, with no task spawned to drive it, so a
  disconnected client (the body stream dropped) tears the subscription down
  for free — there is no background task that could leak.
- Risk: an event relay that bypasses store scope resolution would become a
  scope-leak vector. Addressed for the landscape relay: every emitted event
  is a fresh `landscape().view()` read (the same store-enforced, repo-scoped
  read `GET /landscape/:repo` already uses), never a forwarded raw
  notification. Not yet addressed for memory — see Scope correction above;
  the memory relay remains future work specifically because this guard is
  harder to get right there.
- Rollback: the console degrades gracefully to the ADV-CONSOLE-001 behavior
  (manual Refresh) if the stream endpoint is removed or unavailable — proven
  by construction: `RootApp`'s Refresh button still performs its original
  `fetch_landscape` call independent of the subscription.

## Evidence

- [x] tidy:preparatory — `handlers::landscape_git_repo` extraction
- [x] tdd:red-green — `LandscapeRepository::watch()` (store) and the gateway
      relay endpoint were both driven from a failing test first, verified to
      fail for the right reason (missing method / 404) before implementation;
      see Changes Made for the paired test/feat commits
- [x] tests:unit — `sse::frame` (gateway), `parse_landscape_event` (console)
- [x] tests:integration — `tests/landscape_events.rs` (4 tests, real
      composed `Router`, embedded `kv-mem` store)

## Reviewability

`arrive score --base origin/advance/phase-008` reports **69 [RED]** (Size 50,
Novelty 14, Risk 5 — concurrency, from `Cargo.lock`), 13 files / +706/-39
across three components (store, gateway, console).

Not split, for a reason specific to this advance rather than a blanket
preference for large diffs: the three components form one indivisible
vertical slice — a store-level live-query trigger with no consumer, a
gateway relay with no test proving it actually relays anything without the
store method, or a console subscription with no endpoint to subscribe to —
are each individually *less* reviewable and *not independently testable* on
their own, not more. Splitting would have meant landing an unused
`LandscapeRepository::watch()` in one PR, an untestable relay endpoint in a
second, and dead browser code pointed at nothing in a third — three
incomplete surfaces instead of one working, fully-tested one. The size is
real and the review-time estimate (127 min) is taken as an honest signal for
the reviewer, not argued away.

## CI Evidence Notes

- If CI jobs are enabled, link pipeline evidence (`ci:passed`) from PR/MR and
  default-branch runs.
- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CONSOLE-003 --status passed`

## Changes Made

### 2026-08-07 - test: add a failing test for the landscape watch() trigger
- crates/totem-store/src/landscape.rs: `a_committed_sync_wakes_a_watch_subscriber`,
  failing (E0599/E0282: `watch`/`LandscapeChanges` did not exist)
- crates/totem-store/Cargo.toml: `futures` dependency, `tokio` dev-dep `time` feature

### 2026-08-07 - feat: add LandscapeRepository::watch() as the relay's trigger
- crates/totem-store/src/landscape.rs: `LandscapeChanges`, `LandscapeRepository::watch()`
  — merges one `LIVE SELECT` per landscape table into a payload-free pulse stream

### 2026-08-07 - test: add failing tests for the landscape SSE relay
- crates/totem-gateway/tests/landscape_events.rs: 4 tests against `GET /landscape/:repo/events`
  (did not exist yet — every test 404s)
- crates/totem-gateway/src/dto.rs: `LandscapeEventsQuery`
- crates/totem-gateway/Cargo.toml: `async-stream` dependency, `tokio` dev-dep `time` feature

### 2026-08-07 - feat: implement the landscape SSE relay
- crates/totem-gateway/src/sse.rs: `frame()` — the `text/event-stream` formatter, + unit test
- crates/totem-gateway/src/ops.rs: `LandscapeEventsInput`, `landscape_view`, `log_landscape_read`
- crates/totem-gateway/src/handlers.rs: `landscape_events` handler; extracted
  `landscape_git_repo` (tidy) shared with `landscape`
- crates/totem-gateway/src/lib.rs: registered `GET /landscape/:repo/events`

### 2026-08-07 - fix: assert the relay's second event by membership, not position
- crates/totem-gateway/tests/landscape_events.rs: see Bug Fixes above

### 2026-08-07 - feat: subscribe the console's landscape signal to the live relay
- crates/totem-console/src/view_model.rs: `parse_landscape_event` + test proving it
  parses identically to `parse_landscape`
- crates/totem-console/src/api.rs: `subscribe_landscape_events` (wasm32-only,
  `gloo_net::eventsource::futures::EventSource`), wired into `RootApp` via `use_future`
- crates/totem-console/Cargo.toml: `futures` dependency (wasm32 target)

### 2026-08-07 - tidy: cargo fmt
- crates/totem-gateway/src/lib.rs, crates/totem-gateway/tests/landscape_events.rs

## Check for Understanding

1. `LandscapeRepository::watch()` (`crates/totem-store/src/landscape.rs`)
   merges one `LIVE SELECT` per landscape table into a single pulse stream
   that carries no payload. Why is it safe for that trigger to be unfiltered
   by repo, when the memory table's scope isolation could never be
   unfiltered the same way? What does `ops::landscape_view` do on every
   pulse that makes the difference?
2. `handlers::landscape_events` reads the landscape view (`ops::landscape_view`)
   *before* calling `ops::authorize_repo`. Why is that ordering safe here —
   what does it do, and not do, with that pre-authorization read — and how
   does it match the ordering `handlers::landscape` already used?
3. The relay's whole subscription lifecycle lives inside the
   `async_stream::stream!` body in `handlers::landscape_events`, with no
   `tokio::spawn`. What happens to the `LandscapeChanges` subscription and
   the loop driving it when a client disconnects, and why does that avoid
   the task-leak risk the advance's Risk + Rollback section names?
4. `tests/landscape_events.rs`'s `a_later_write_pushes_a_second_event_with_the_updated_view`
   test was fixed mid-advance (commit `8d9f405`) to check `advance_ids(...).contains(...)`
   instead of `advances[0].id`. Why does a second `/enroll` call with a
   different advance id not replace the first one in `LandscapeRepository::sync`,
   and what would the original (position-based) assertion have gotten wrong
   about what the relay was actually proving?
5. `view_model::parse_landscape_event` in `crates/totem-console/src/view_model.rs`
   is a one-line delegation to `parse_landscape`. What does its test
   (`a_relayed_landscape_event_parses_identically_to_a_polled_fetch_body`)
   actually verify, given the function does nothing beyond calling
   `parse_landscape`?
6. The Scope correction section says the memory-table live relay (recall
   polling) was deferred entirely, not partially implemented. What
   store-enforced guarantee does a scope-correct memory relay need that the
   landscape relay in this advance does not, and why does that make it
   materially more work than what shipped here?
