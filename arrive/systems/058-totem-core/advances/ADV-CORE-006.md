---
advance:
  id: "ADV-CORE-006"
  title: "Auth refusals join the access log"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store", "gateway"]
  started_at: "2026-08-06T13:45:00Z"
  implementation_completed_at: "2026-08-07T04:51:15Z"
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 80
  risk_flags: ["auth", "migration"]
  evidence: ["tdd:red-green", "tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: complete
---

## Objective

Close the audit gap ADV-GATEWAY-003 disclosed as a partially established
invariant: refused requests append nothing to the access log. The gateway
component's "no unlogged access" invariant currently covers every *successful*
read and write; a credential probing the boundary — wrong repo, expired
token, scope it doesn't own — is precisely the event a security audit wants
to see, and today it leaves no trace.

Wired as a dependency of ADV-GATEWAY-006 (the security evaluation must find
refusals in the log, not flag their absence).

## Behavioral Change

After this advance:

- `AccessOperation` (totem-core) gains a refusal variant carrying what was
  presented and why it was refused (the `AuthError` kind, the route, the
  fingerprint of the presented credential if any — never the token text).
- The gateway's auth layer appends a refusal entry for every request it
  turns away, on every route, including the MCP surface — error paths, not
  just the happy path, per the component hazard note.
- Refusal entries are queryable alongside the rest of the access log and are
  append-only like everything else in it.
- A test proves a refused request leaves exactly one refusal entry, and a
  control proves the entry appears even when the store itself is never
  reached (the refusal happens above the store — the log write is the only
  store touch).

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change) — **skipped, and
      here is why.** There was nothing pre-existing to tidy ahead of this
      change: refusal logging did not exist anywhere in the gateway to
      extract or de-duplicate in advance. The one piece of genuine
      duplication this advance removes (the repeated
      `caller.authorize_identity(...)?`/`authorize_scope(...)?` shape across
      every `ops::` function) is inseparable from the new behavior — the
      whole point of the `authorize_identity`/`authorize_scope`/
      `authorize_repo` helpers is to log on the failure path, so extracting
      them *is* the feature, not preparation for it. No tidy commit exists on
      the sub-branch.
- [x] test: refusal entries per route (REST + MCP), no-token and bad-token
      shapes, append-only holds for refusal entries
- [x] feat: the `AccessOperation` variant, store write path, gateway wiring

## Bug Fixes

- [ ] None

## Scope and Boundaries

**In scope:** the refusal variant, its store write, gateway wiring on all
routes, tests.

**Out of scope:** rate limiting or lockout on repeated refusals (a later
curator/infra concern); repo-binding itself (ADV-GATEWAY-009); console
surfacing of refusal entries (belongs with ADV-CONSOLE-002's audit views).

## Risk + Rollback

- Risk (`auth` flag): the refusal path must not become a write amplifier —
  an unauthenticated request now causes a store write. Kept minimal (one row,
  no retries): the flood concern is noted, not solved — a credential-less
  caller hammering `/save` now produces one refusal row per attempt, and
  nothing here rate-limits or coalesces them. Left for the rate-limiting
  follow-up the advance's own Scope and Boundaries names.
- Risk (`migration` flag): migration 9 widens `access_log.actor`/`harness`/
  `session` from required to `option<string>` and widens the `operation`
  assertion, using the same `OVERWRITE` technique migrations 4 and 7 already
  established — additive, and `migrations_apply_once_and_replay_as_a_no_op`
  / `migration_versions_are_ordered_and_unique`
  (`totem-store/tests/schema_contracts.rs`) cover it generically, with no
  hardcoded migration count to update.
- Risk: logging must never turn a refusal into a success. Verified two ways:
  every refusal path logs *before* returning the error (never after a
  success path), and a log-write failure is caught and reported via
  `eprintln!` rather than propagated — `log_refusal`'s and `authenticate`'s
  own `if let Err(log_error) = ... { eprintln!(...) }` shape in
  `crates/totem-gateway/src/auth.rs`. Not exercised by an integration test
  (that would need an injectable store failure this codebase has no seam
  for yet), so this is honesty-based, not test-proven — stated plainly per
  this advance's own instruction.
- Rollback: revert branch; refusals return to unlogged, the recorded
  residual state. The migration is additive and not itself reverted by a
  code rollback — an already-migrated database keeps the wider schema, which
  is harmless (a `refused` row simply stops being written).

## Reviewability

`arrive score --base origin/advance/phase-008` reports **80 [RED]** (size 60,
risk 20; flags `auth`, `migration`). The budget says split at Red or document
why the change is atomic. **Documented, not split.**

**Why it does not split into shippable halves.** The natural cut line is
"the `totem-core` type change" vs. "the gateway wiring that uses it", but
neither half stands alone:

- *Core/store first, gateway later* leaves `AccessOperation::Refused` and
  `RefusalReason` dead code — nothing constructs a refusal entry, so the
  migration and the new columns exist unused until a second PR lands, and
  reviewers of the first PR cannot see the invariant it is supposedly
  building toward.
- *Gateway first, core/store later* does not compile: the gateway's
  refusal-logging call sites (`auth::log_refusal`, the `ops::authorize_*`
  helpers) are written directly against `AccessOperation::Refused` and
  `RefusalReason` — there is no gateway-only stub shape that would let this
  land first.

The size is also concentrated in one honest place: of the 846 inserted
lines, roughly 230 are the new `tests/auth.rs` refusal assertions (the part
a reviewer of a security-audit change should want *more* of), ~90 are the
`ops.rs` mechanical transform (every existing `caller.authorize_*(...)?`
call site gained an `endpoint` argument and an `.await`), and the rest is
the type/schema change plus its own doc comments. No individual call site
needs more than a glance; the breadth is what makes "a future route forgot
to log its refusal" fail to compile rather than fail silently, the same
argument ADV-GATEWAY-003 made for its own comparable width.

**Suggested review order:** `crates/totem-core/src/access_log.rs` (the
shape) → `crates/totem-store/src/schema.rs`'s migration 9 doc comment (why
`OVERWRITE`, why `option<string>`) → `crates/totem-gateway/src/auth.rs`
(`log_refusal`, `authenticate`) → `crates/totem-gateway/tests/auth.rs`'s new
"Refusals are logged" section (what is actually proven) → `ops.rs`/
`handlers.rs`/`mcp.rs`, which are mechanical once the first four make sense.

## Evidence

- [ ] tidy:preparatory — deliberately not claimed; see the Planned
      Implementation Tasks entry above for why no tidy commit exists.
- [x] tdd:red-green — the test commit (`7f0fcd3`) precedes the implementation
      commit (`63c15a5`). Red was captured as a real compile failure on the
      committed test tree, the same shape ADV-GATEWAY-003 used:

      error[E0599]: no function or associated item named `refused` found
      error[E0433]: failed to resolve: use of undeclared type `RefusalReason`
      error[E0599]: no variant or associated item named `Refused` found for enum `AccessOperation`

- [x] tests:unit — 6 new/updated in `totem-core/src/access_log.rs`'s own
      test module (a refusal entry's shape, its JSON round trip).
- [x] tests:integration — `totem-store/tests/access_log.rs` (a refusal
      round-trips through the store with no actor/harness/session);
      `totem-gateway/tests/auth.rs` gained 5 tests proving refusal logging
      for missing/unknown/expired credentials, an authorization refusal
      (wrong actor), and the MCP surface, each asserting the refusal is the
      request's *only* store touch (`entries.len() == 1`). `cargo test
      --workspace` is fully green (every crate, including
      `totem-gateway/tests/recall_and_save.rs` and
      `totem-curator/tests/dedupe.rs`, both of which needed their own
      pre-existing assertions updated for `actor`/`harness` becoming
      `Option`).
- `ci:passed` is **not** claimed: no pipeline result was read from this run.

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7.

## Changes Made

### 2026-08-07 - test: Refusal entries per route, no-token and bad-token shapes

- crates/totem-core/src/access_log.rs: tests for a refusal entry's shape (no
  identity fields, refusal reason, optional fingerprint) and its JSON round
  trip — fail to compile against `AccessOperation::Refused`,
  `RefusalReason`, `AccessLogEntry::refused` before the feat commit
- crates/totem-store/tests/access_log.rs: a refusal entry round-trips
  through the store with no actor/harness/session
- crates/totem-gateway/tests/auth.rs: `app_with_state()` helper; refusal
  logging tests for missing/unknown/expired credentials and an
  authorization refusal (wrong actor), each proving the log write is the
  refused request's only store touch, plus one MCP-surface refusal test
- crates/totem-gateway/tests/recall_and_save.rs,
  crates/totem-curator/tests/dedupe.rs: pre-existing assertions updated for
  `actor`/`harness` becoming `Option`

### 2026-08-07 - feat: Refusal entries join the access log

- crates/totem-core/src/access_log.rs, lib.rs: `AccessOperation::Refused`;
  `RefusalReason` (missing/unknown/expired credential, actor/repo/scope not
  bound) — `totem-core`'s own vocabulary, not a re-export of any surface's
  error type; `AccessLogEntry::actor`/`harness`/`session` become `Option`;
  `AccessLogEntry::refused()` and `.with_fingerprint()`
- crates/totem-store/src/schema.rs: migration 9
  (`ACCESS_LOG_REFUSAL_SCHEMA_V9`) — widens `actor`/`harness`/`session` to
  `option<string>` and `operation`'s assertion to include `'refused'`
  (`OVERWRITE`, migrations 4/7's technique); adds `refusal_reason` and
  `credential_fingerprint` columns
- crates/totem-store/src/migrate.rs: registers migration 9
- crates/totem-store/src/access_log.rs: `to_row`/`from_row` handle the
  optional identity columns and the two new fields;
  `refusal_reason_key`/`refusal_reason_from` mirror `operation_key`/
  `operation_from`'s existing pattern
- crates/totem-gateway/src/auth.rs: `Caller::Bound` now carries the
  credential's hex fingerprint alongside its grant;
  `AuthError::refusal_reason()` maps the request-refusal variants to
  `RefusalReason` (`None` for the two issue-time-only variants);
  `fingerprint_hex()`; `log_refusal()` (best-effort, never turns a refusal
  into a success); `authenticate()` logs before returning 401, using the
  presented (even if invalid) token's own fingerprint when `verify` itself
  fails
- crates/totem-gateway/src/ops.rs: `authorize_identity`/`authorize_scope`/
  `authorize_repo` helpers wrap every existing `caller.authorize_*` check
  with `log_refusal`; every operation's own check now routes through them
- crates/totem-gateway/src/handlers.rs: `enroll`'s two repo-binding checks
  (including the ARRIVE-id rebind guard) and `landscape`'s now route through
  the logged helpers
- crates/totem-gateway/src/mcp.rs: `TotemMcp::caller()` is `async` and logs
  its own defense-in-depth refusal when no verified `Caller` extension is
  found (unreachable in ordinary operation — the shared `authenticate`
  middleware refuses first — but now traceable if a future route ever
  reaches it)
- crates/totem-gateway/src/lib.rs: `authenticated_app`'s middleware layer
  is built over the whole `AppState`, not just the token registry, so
  `authenticate` can reach the store
- arrive/systems/058-totem-core/components/gateway.yaml: new invariant —
  refused requests append to the access log too

## Check for Understanding

1. `AccessLogEntry::actor`/`harness`/`session` became `Option<...>` rather
   than adding a fourth "refused" struct alongside the existing one. What
   broke at compile time because of that choice (see
   `crates/totem-curator/tests/dedupe.rs` and
   `crates/totem-gateway/tests/recall_and_save.rs`), and why is that
   breakage the point rather than a cost to work around?

2. `authenticate()` in `crates/totem-gateway/src/auth.rs` handles a missing
   token and an invalid one (`verify()` returning `Err`) with two different
   code paths instead of both calling `log_refusal()`. What does the invalid
   case have that the missing case does not, and why does that difference
   make sharing the same helper impossible?

3. `ops::authorize_identity`/`authorize_scope` and `handlers::enroll`'s
   authorization checks now both call into functions that append to the
   access log — but `ops::save`'s own successful path already appends a
   *separate* `AccessOperation::Save` entry further down the same function.
   Walk through what happens, log-entry by log-entry, when
   `caller.authorize_scope` refuses a `save` call. How many entries does the
   access log gain, and why is that the same answer this advance's
   `a_request_with_no_credential_leaves_exactly_one_refusal_entry` test
   proves for a different refusal?

4. `TotemMcp::caller()`'s own refusal-logging branch is described in this
   advance as "unreachable in ordinary operation". Trace why: which
   composition in `crates/totem-gateway/src/lib.rs` is the only one that
   constructs a `McpAuth::TokenBound` handler, and what does that
   composition guarantee has already run before any MCP tool call reaches
   `caller()`?

5. Migration 9 widens `access_log.actor` from `TYPE string ASSERT $value !=
   ''` to `TYPE option<string> ASSERT $value = NONE OR $value != ''` using
   `OVERWRITE`. Why does a row written under migration 2 (before this
   advance existed) still satisfy the widened assertion without any
   backfill statement — and what would go wrong if migration 9 had instead
   tried to *narrow* an existing assertion this way?

6. `RefusalReason` in `totem-core` and `AuthError` in `totem-gateway` name
   overlapping cases (missing credential, expired, scope not bound). Why
   does `totem-core` define its own enum instead of `totem-gateway`
   constructing `AccessLogEntry`s directly with its own `AuthError`
   embedded in them? What would that alternative have required of
   `totem-core`'s dependency graph?
