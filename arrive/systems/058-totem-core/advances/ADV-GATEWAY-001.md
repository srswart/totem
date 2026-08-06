---
advance:
  id: "ADV-GATEWAY-001"
  title: "Axum REST API: recall/save with provenance + access log"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T03:37:03Z"
  review_time_estimate_minutes: 192
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 105
  risk_flags: ["public_api", "migration", "new_dependency", "concurrency"]
  evidence: ["tidy:preparatory", "tdd:red-green", "tests:unit", "tests:integration"]
  # Populated by `arrive usage import` / the LiteLLM callback — leave empty when authoring.
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

Expose the first HTTP surface in `totem-gateway`: recall (merged, ranked
context across the scope chain) and save (write with provenance auto-attached),
with every read and write appended to the access log (Solution Intent §3.2, §4).

## Behavioral Change

After this advance:
- `POST /recall` returns merged, scope-resolved context for a query;
  `POST /save` persists a memory with provenance attached from the caller's
  claimed identity (author, harness, session — see "Partially established
  invariants" below: this is not yet an *authenticated* identity).
- Every request that reaches a handler and completes — including a recall that
  returns nothing — appends one access-log record (who, what, when, via which
  endpoint, in which session), verifiable via `store.access_log().list()`.
- Embedding happens on the gateway, on write and on recall query text
  (docs/tech-direction/embeddings.md §4's placement decision), using
  `totem-store`'s `DeterministicEmbedder` by default — the real model
  (`fastembed`/BGE-small-en-v1.5, EMB-004) needs a model download this
  sandbox's egress policy blocks, so it stays behind `totem-store`'s
  off-by-default `fastembed` feature, exactly as it already did before this
  advance.
- The access log itself is append-only at the database level: a new migration
  (version 2) adds `access_log` with the same `EVENT`-refuses-`UPDATE`/`DELETE`
  pattern `memory` already uses for episodic rows, so a tampered audit trail is
  refused by SurrealDB, not merely undocumented by this crate's API.
- The gateway stays thin over the core API: every DTO in `totem-gateway::dto`
  is built directly on `totem-core`'s own `Serialize`/`Deserialize` types
  (`Scope`, `Author`, `MemoryCategory`, the validated id newtypes) rather than
  a parallel set of gateway-only structs, so request validation is the same
  validation `totem-core` and `totem-store` already enforce — not a second
  copy of it (mitigation for the brief's "standard drift" risk).
- **Connection constraint (TD-009, executed):** the gateway's own SurrealDB
  connection must be embedded or WebSocket — never the HTTP protocol, which
  refuses live queries; the console's live feeds depend on this. The `main.rs`
  binary uses the embedded engine (`Store::in_memory`), which also means it is
  **not persistent across restarts** — production deployment topology
  (embedded vs. server, where state survives a restart) is an open question
  (Solution Intent §9) this advance does not resolve; see docs/tech-direction/surrealdb.md
  §5.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: widen `totem-store/src/row.rs`'s conversion helpers to
      `pub(crate)` so the new access log repository reuses them instead of
      duplicating field-by-field mapping (no behavior change)
- [x] test: access log entries/repository, gateway DTOs/router/error mapping,
      and `/recall`+`/save` integration tests (red first)
- [x] feat: `AccessLogEntry`/`AccessOperation` (totem-core), `AccessLogRepository`
      + migration 2 (totem-store), and the `totem-gateway` crate — router,
      handlers, error mapping, binary

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: API shapes become de-facto contracts for MCP tools and console; access
  logging gaps would undermine G3 (auditability).
- Risk (realized, documented rather than fixed here — see "Partially
  established invariants" below): a **refused** `/save` (`StoreError::ScopeDenied`)
  is not appended to the access log today, only completed reads/writes are.
  CLAUDE.md's access-log hazard specifically calls out checking error paths;
  this one is a genuine gap, kept out of this advance's scope rather than
  silently left unmentioned.
- Risk (realized, by design, deferred): `author`/`harness`/`session` are
  caller-supplied JSON fields, not derived from an authenticated credential.
  Until ADV-GATEWAY-003 lands token auth, any caller can claim to be any
  actor — scope isolation at the store layer still holds (a claimed actor
  cannot write outside its own claimed chain), but identity itself is
  unverified.
- Rollback: revert branch; endpoints are additive, migration 2 only adds a new
  table (`access_log`) alongside the existing `memory` table — no existing
  schema element changes, so rollback drops nothing another advance depends on.

## Reviewability

`arrive score` reports **105 [RED]** (size 70, novelty 20, risk 15: `migration`,
`concurrency`). Documented rather than split, for the same reason ADV-STORE-001
gave for its own Red score: every plausible split ships an unenforced
invariant, here specifically the access log's own claim.

- `AccessLogRepository` without a caller (the gateway) is a persistence layer
  nothing exercises — reviewable in isolation the same way "schema without
  repositories" was for ADV-STORE-001, and for the same reason: it proves
  nothing about the invariant it exists to serve.
- The gateway's `save`/`recall` handlers without the access log calls would
  ship the API surface while leaving G3 ("every read and write is
  audit-logged") false for the one surface that exists to demonstrate it.
- Splitting `/save` from `/recall` into separate sub-PRs would prevent the
  round-trip test (`saving_and_recalling_round_trips_over_http`) and the
  cross-actor isolation test (`recall_never_returns_another_actors_private_memory`)
  from existing at all — the latter is the project's highest-severity
  invariant, now proven end-to-end over real HTTP for the first time.
- Tests split from either half stop being able to fail.

Two-thirds of the size score is mechanical, matching ADV-STORE-001's own
breakdown: `access_log.rs`'s row (de)serialization in `totem-store` is a
field-by-field mapping table shaped exactly like `row.rs`'s existing one for
`MemoryRecord`, and `totem-gateway/tests/recall_and_save.rs` (∼200 lines) is
fixtures and assertions, not branching logic.

What was *not* pulled in to keep it this size: an admin/audit REST endpoint to
read the access log (ADV-CONSOLE-002's "audit trails" scope; today the log is
verified via `store.access_log().list()` directly, not over HTTP), the MCP
surface (`totem_recall`/`totem_save`/`totem_landscape`, ADV-GATEWAY-002), and
authentication (ADV-GATEWAY-003).

## Evidence

- [x] tidy:preparatory — `tidy:` commit widens `row.rs` helper visibility with
      no behavior change; the pre-existing store suite passes unchanged
- [x] tdd:red-green —
      - `totem-gateway`: confirmed by execution. `cargo test -p totem-gateway`
        failed 7 of 8 tests with `not implemented: ADV-GATEWAY-001` before the
        `feat:` commit; all 8 pass after.
      - `totem-store`'s access log: confirmed by an isolated compile check —
        `access_log.rs`, its `lib.rs` wiring, and `Store::access_log()`
        reverted to their pre-advance state, `cargo build -p totem-store --tests`
        fails with `no method named access_log found for struct Store<C>`;
        restored, it builds and the 3 integration tests plus 2 in-crate
        append-only tests pass.
- [x] tests:unit — `totem-core::access_log` (3 tests); `totem-store::schema::tests`
      gains 2 tests proving the access log table refuses raw `UPDATE`/`DELETE`,
      mirroring the existing episodic tests
- [x] tests:integration — `totem-store/tests/access_log.rs` (3 tests: record→list
      round trip, a save entry's `memory_id`, oldest-first ordering);
      `totem-gateway/tests/recall_and_save.rs` (8 tests, real HTTP via
      `tower::ServiceExt::oneshot`, no bound socket)
- [ ] ci:passed — **not claimed.** CI runs on `advance/**` pushes; this record
      was written before the pipeline result for this branch was available.

Workspace verification at the time of writing: `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and
`cargo test --workspace` all clean (every crate's suite passing, gateway's
8 integration tests included); `arrive doctor artifacts` 0 issues;
`arrive plan check` valid. The `totem-gateway` binary was also smoke-tested
manually: built, started, and driven with `curl` against `/save` then
`/recall` over a real bound TCP socket (not `oneshot`) — the saved record came
back with its embedding and provenance intact.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-GATEWAY-001 --status passed`

## Partially established invariants

Stated plainly rather than implied by a green suite:

1. **"Every read and write appends to the access log" holds only for
   operations that reach the store and complete.** A `/save` refused with
   `StoreError::ScopeDenied` — a caller-identified actor attempting to write
   outside its own claimed chain — is not logged. CLAUDE.md's access-log
   hazard explicitly calls for checking error paths, not just the happy path;
   this is exactly that gap, found and named rather than silently left
   unmentioned. `/recall` has no equivalent gap: a reader's chain always
   yields a (possibly empty) result rather than an error, and an empty recall
   *is* logged (`recalling_an_unknown_memory_never_happened_still_appends_an_access_log_entry`).
2. **Identity is claimed, not authenticated.** `author`, `harness`, and
   `session` come from the request body. The store still enforces that a
   claimed writer cannot write outside its own claimed scope chain, so this is
   not a scope-isolation bypass — but nothing today stops a caller from
   claiming to be a different actor than it is. ADV-GATEWAY-003 ("Streamable-HTTP
   MCP + auth for cloud agents") is where this closes, per its own advance and
   CLAUDE.md's security-critical list.
3. **The binary is not a deployable service yet.** `main.rs` connects
   `Store::in_memory()` — correct for TD-009's embedded-or-WebSocket
   constraint, but state does not survive a restart. Solution Intent §9 leaves
   deployment topology open; resolving it is not this advance's job.
4. **Recall's `query` text uses whatever embedder the caller's `AppState` was
   built with**, which is `DeterministicEmbedder` (non-semantic, trigram
   hashing) in both the default test build and the shipped `main.rs` binary.
   `a_text_query_still_returns_readable_matches` proves the query path is
   wired end-to-end, not that ranking is semantically meaningful — EMB-004
   measured the real model's quality on a workstation, not in this sandbox,
   and that finding is unchanged by this advance.
5. **No REST endpoint reads the access log.** The objective's "verifiable by
   an audit query" is satisfied at the store layer
   (`store.access_log().list()`, exercised directly by tests) — an HTTP
   surface for it is ADV-CONSOLE-002's "audit trails" scope, not this one's.

## Corrections to this advance

The advance as originally authored said access logging happens "with every
read and write appended to the access log" without qualifying "completes
successfully" — see Partially established invariants #1 above for the one
case (`ScopeDenied` on `/save`) where that is not yet true. No other factual
error was found; the TD-009 reference held, and the access-log table follows
the same append-only pattern `docs/tech-direction/surrealdb.md` already
established for episodic memory (this advance did not need to invent a new
enforcement mechanism, only apply the existing one to a second table).

## Changes Made

### 2026-08-06 - tidy: widen totem-store row.rs helpers to pub(crate)
- `crates/totem-store/src/row.rs`: `malformed`, `memory_id`, `harness_key`,
  `harness_from`, `field`, `string`, `record_id`, `datetime`, `number` widen
  from private-to-module to `pub(crate)`, unchanged bodies — the access log
  repository reuses them instead of a second copy

### 2026-08-06 - test: access log entries, gateway scaffold, recall/save integration tests
- `crates/totem-core/src/access_log.rs`: `AccessLogEntry`, `AccessOperation`,
  builder methods (`at_turn`, `for_memory`, `with_result_count`), 3 unit tests
- `crates/totem-core/src/lib.rs`: exports the new module
- `crates/totem-store/src/schema.rs`: `ACCESS_LOG_SCHEMA_V2` (append-only
  `access_log` table) plus 2 in-crate tests proving raw `UPDATE`/`DELETE` are
  refused
- `crates/totem-store/src/migrate.rs`: registers migration version 2
- `crates/totem-store/tests/access_log.rs`: 3 integration tests against the
  public API only
- `crates/totem-gateway/`: new crate — `Cargo.toml`, `dto.rs` (`SaveRequest`/
  `SaveResponse`/`RecallRequest`/`RecallResponse`, built on `totem-core`
  types), `error.rs` (`GatewayError` → HTTP status mapping), `state.rs`
  (`AppState`), `lib.rs` (router wiring), `main.rs` (binary), `handlers.rs`
  (both handlers `unimplemented!("ADV-GATEWAY-001")`), and
  `tests/recall_and_save.rs` (8 integration tests) + `tests/common/mod.rs`
  (fixtures, `tower::ServiceExt::oneshot` driver)
- `Cargo.toml`, `Cargo.lock`: `totem-gateway` joins the workspace

### 2026-08-06 - feat: access log repository, save/recall handlers
- `crates/totem-store/src/access_log.rs`: `AccessLogRepository` (`record`/`list`),
  row (de)serialization reusing the widened `row.rs` helpers
- `crates/totem-store/src/store.rs`, `src/lib.rs`: `Store::access_log()` accessor
- `crates/totem-gateway/src/handlers.rs`: `save` — resolves the writer's
  `ScopeChain`, embeds content, constructs `Provenance`, persists, appends an
  access-log entry; `recall` — resolves the reader's chain, optionally embeds
  `query` text as a vector probe, applies category/since/limit filters,
  appends an access-log entry (including on an empty result)

## Check for Understanding

1. `handlers::save` calls `state.store.access_log().record(&entry)` only
   *after* `state.store.memories().save(&writer, &record).await?` succeeds.
   Trace what happens to the access log when `save` returns
   `StoreError::ScopeDenied`, and say which line in `handlers.rs` is
   responsible for that gap (see "Partially established invariants" #1).
2. `dto.rs`'s `SaveRequest` and `RecallRequest` are built directly on
   `totem-core` types (`Scope`, `Author`, `MemoryCategory`, `SessionId`, …)
   rather than a parallel set of gateway-only DTOs with their own validation.
   Trace what happens end-to-end when a caller POSTs `"scope": "not-a-scope"`
   to `/save` — which type's `Deserialize` impl rejects it, and does
   `handlers::save` ever run?
3. `error.rs` maps `StoreError::ScopeDenied` to `403 Forbidden` but
   `StoreError::NotFound` to `404 Not Found`, even though `totem-store`'s own
   design (ADV-STORE-001) deliberately makes a foreign record read as *absent*
   rather than *forbidden* to prevent enumeration. Explain why `ScopeDenied`
   on a **write** does not carry the same enumeration risk a **read** miss
   does, and why the two error variants therefore get different HTTP
   treatment here.
4. `state.rs`'s `AppState` holds `embedder: Arc<dyn Embedder>`, and both
   `handlers::save` and `handlers::recall` call `state.embedder.embed(...)`
   with the *same* embedder. Given `main.rs` constructs a
   `DeterministicEmbedder`, what does `a_text_query_still_returns_readable_matches`
   actually prove, and what would it take to know whether the returned record
   is a *good* match for the query rather than merely *a* match?
5. `crates/totem-store/src/schema.rs`'s `ACCESS_LOG_SCHEMA_V2` gives
   `access_log` the same two `DEFINE EVENT ... THROW` statements `memory`
   already has for episodic rows. `crates/totem-store/tests/access_log.rs`
   does not itself test this refusal — where does that test live instead, and
   why does it need direct connection access
   (`crates/totem-store/src/store.rs`'s `#[cfg(test)]` accessor) that the
   gateway's own tests never get?
6. The Reviewability section argues this Red-scoring (105) advance should not
   be split further. Pick one candidate split from that section (e.g.
   `access_log.rs` landing without the gateway calling it) and say
   specifically which test in `tests/recall_and_save.rs` would have nothing
   to exercise if that split had happened.
