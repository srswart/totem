---
advance:
  id: "ADV-CONSOLE-002"
  title: "Audit trails, Uncertainty queue, promotion approvals"
  system: "058-totem-core"
  primary_component: "console"
  components: ["console", "gateway", "core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T18:30:00Z"
  review_time_estimate_minutes: 156
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 86
  risk_flags: ["auth", "migration", "public_api"]
  evidence: ["tidy:preparatory", "tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

Governance surfaces in the console (Solution Intent §5): audit trail viewer
(any memory's provenance and access history on demand — G3/success measure
"auditability"), the contested-memory (Uncertainty) queue with resolution
recording, and promotion approvals for human-gated scope promotions.

**Scope corrected 2026-08-06** (see `## Scope Correction` below): the
Objective's fourth item, "value/usage reports incl. the 'retire?' queue fed
by ADV-CORE-002", is not delivered by this run and is left as an unassigned
gap rather than silently dropped.

**Component correction 2026-08-06:** `components` was authored as
`["console", "gateway"]`. Implementing the Uncertainty queue's resolution
step and the audit trail's access-history read honestly needed new
`totem-core`/`totem-store` additions (`Governance::resolve`,
`MemoryRepository::pending_review`/`resolve_review`,
`AccessLogRepository::for_memory`), not just gateway plumbing over
already-built store methods — the same shape ADV-GATEWAY-004's own
correction described. Corrected above rather than narrowing scope to fit
the original component list.

## Behavioral Change

After this advance:

- A reviewer can reconstruct any memory's lineage from the console's Audit
  tab: its provenance (author, harness, session, time), its full access
  history (`GET`-equivalent `POST /audit/:id`, backed by the new
  `AccessLogRepository::for_memory`), its curator lineage (merges and
  rollbacks, via the `CurationRepository::history` ADV-CURATOR-001 already
  built with this advance in mind), and its promotion history (via
  `PromotionRepository::history`, likewise pre-built by ADV-CORE-003).
- Contested memories (Uncertainty-category records, created with
  `governance.review = Pending` because Uncertainty is human-gated by its own
  category lifecycle — no separate "contested" flag needed) sit in a queue
  (`POST /uncertainty/pending`, backed by the new
  `MemoryRepository::pending_review`) until a human resolves them
  (`POST /uncertainty/:id/resolve`, backed by the new `Governance::resolve` +
  `MemoryRepository::resolve_review`). The resolution is recorded as the
  record's own governance state (not silently applied and forgotten) and
  logged (`AccessOperation::Resolve`); a decided review cannot be re-decided.
- Human-gated promotions (e.g. Instructions to project/platform scope) are
  proposed, approved, or rejected in the console's Governance tab
  (`POST /promotions`, `POST /promotions/pending`, `POST /promotions/:id/record`,
  `POST /promotions/:id/approve`, `POST /promotions/:id/reject`), each a
  thin gateway wrapper over the `PromotionRepository` ADV-CORE-003 already
  built and proved. Every decision is the recorded event ADV-CORE-003
  defined; approving moves the record, rejecting leaves it in place.
- All six new endpoints follow the same
  resolve-chain/do-the-operation/append-one-access-log-entry pattern
  `ops.rs` already established for `/recall`/`/save`/`/feedback`/`/contest`,
  so they cannot silently skip the access log the way an ad hoc handler
  could.

## Scope Correction

Two pieces of the original Objective are **not** delivered by this run,
disclosed here rather than silently dropped:

1. **Value/usage reports and the "retire?" queue** (ADV-CORE-002's economics
   surfaced for a human to review) — a genuinely separate reporting feature
   from the three governance surfaces above, with its own view model and
   endpoint. Left as an unassigned gap; a follow-up advance should pick it up
   explicitly rather than this run silently expanding to cover it.
2. **`demote` and MCP-tool exposure for propose/approve/reject** — the
   `PromotionRepository::demote` rollback lever and an agent-facing MCP
   surface for proposing/deciding promotions both exist at the store layer
   (`demote`) or don't yet exist at all (MCP), but neither is wired to a
   surface by this run. The Behavioral Change bullets above only require
   propose/approve/reject to be *reachable* from the console (REST), which is
   satisfied; demotion as a console action, and any of this via `totem_*` MCP
   tools, are follow-up work.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change) — `Serialize`/
      `Deserialize` added to `PromotionEvent`/`CurationEvent` and their kind
      enums, needed before either could travel over the gateway's JSON API.
- [x] test: API-contract tests for audit/queue/approval endpoints; view tests
      where supported
- [x] feat: audit viewer, Uncertainty queue, promotion approval views +
      gateway endpoints

## Bug Fixes

- Fixed: `totem-gateway/src/dto.rs`'s `EnrollResponse` had its `components`/
  `advances` doc comments swapped (pre-existing, unrelated to this advance's
  own behavior — noticed while editing the same file for the new DTOs).

## Risk + Rollback

- Risk: approval actions mutate governance state; wrong wiring could
  auto-approve gated promotions. Mitigated by depending on the promotion
  engine (ADV-CORE-003) exactly as designed — this advance adds no new
  policy logic, only REST wrappers around `PromotionRepository::propose`/
  `approve`/`reject`, which already enforce per-category policy and
  scope-reachability. `an_uncontested_project_scope_cannot_be_proposed_into_by_a_non_member`
  and the automatic-vs-human-gated tests in
  `crates/totem-gateway/tests/promotion_uncertainty_audit.rs` exercise this.
- Risk: the Uncertainty resolution guard could be raced (two resolvers
  deciding the same review at once). Mitigated the same way ADV-CORE-003
  documented for promotion decisions: `MemoryRepository::resolve_review`
  re-checks `governance.review = 'pending'` in the resolving `UPDATE`
  itself, not only in the earlier `Governance::resolve` pre-check, so a
  second decision refuses (`StoreError::ReviewDecided`) rather than
  silently overwriting the first. Not further concurrency-tested, the same
  disclosed limit ADV-CORE-003's "Concurrency" note accepted for the
  analogous promotion-decision race.
- Risk: `AccessLogRepository::for_memory` and the new `/audit/:id` endpoint
  are a new way to read data about a record. Mitigated by re-checking
  visibility via `MemoryRepository::get` inside `for_memory` itself (not
  trusting the gateway's own prior check), the same "verify inside the
  store method" pattern `PromotionRepository::propose`/`demote` already use
  — proven by `the_audit_trail_for_a_record_outside_the_readers_chain_is_not_found`
  and `for_memory_on_a_record_outside_the_readers_chain_reads_as_not_found`.
- Rollback: revert this sub-PR's merge commit. Every change is additive (new
  modules, new repository methods, new routes) — nothing here alters
  `totem_recall`/`totem_save`/`totem_feedback`/`totem_contest`'s existing
  behavior or wire shape, so a revert cannot regress them. Migration 7 only
  widens an existing `ASSERT` set (the same non-breaking `OVERWRITE`
  technique migration 4 used), so it needs no down-migration.

## Reviewability

`arrive score` reports **86 [RED]** (Size 72, Novelty 4, Risk 10; flag:
`migration`) over 10 unpushed commits, a review-time estimate of 155.8
minutes. Not split, for the same two reasons ADV-GATEWAY-004 gave for its
own four-endpoint gap-fill (which scored 91 RED): this run
implements exactly one advance per the phase-per-run protocol, and the six
new endpoints (three promotion, two Uncertainty, one audit) do not
decompose cleanly — each pairs one thin `ops.rs` function with one REST
handler and one DTO pair, the same shape `totem_recall`/`totem_save`
already establish, so splitting by endpoint would touch `dto.rs`, `ops.rs`,
`handlers.rs`, and `lib.rs` six times over in near-identical shape without
reducing what a reviewer has to understand in any one diff. The console's
three new views and their wiring are the other natural split point, but
they are inert without the gateway surface they call — landing them apart
would put an approve button in front of a reviewer with no endpoint behind
it.

Mitigating the size for review: the ten-commit series is split by
architectural layer and by test-then-implementation
(tidy → core test+feat → store test → store feat → gateway test → gateway
feat → console view-model → console views → console wiring), so a reviewer
can verify layer by layer rather than as one diff, and every `feat:` commit
lands after the test commit that was written to fail against it.

**TDD note, honestly:** core's `Governance::resolve` and the store's
`pending_review`/`resolve_review`/`for_memory` were genuinely red-first —
their test commits fail to compile against the not-yet-existing methods,
verified before the implementation commit landed. The gateway's ten REST
tests were likewise committed before the endpoints existed and failed to
compile. The console's view-model and view tests were written and
committed together with their implementation in the same commit, not
strictly red-first, because they mechanically mirror the already-proven
gateway wire shapes — `tdd:red-green` is therefore claimed for the
core/store/gateway layers and not claimed for the console layer.

## Evidence

- [x] tidy:preparatory — one tidy commit (`Serialize`/`Deserialize` derives),
      no behavior change, full suite green before and after.
- [x] tdd:red-green — core (`Governance::resolve`'s tests), store
      (`pending_review`/`resolve_review`/`for_memory`'s tests), and gateway
      (the ten `promotion_uncertainty_audit.rs` tests) each failed to
      compile before their implementation commit. Not claimed for the
      console layer — see the honest TDD note above.
- [x] tests:unit — `cargo test --workspace`: every crate green, including
      47 `totem-core` tests (3 new: `Governance::resolve`'s three cases,
      plus round-trip tests for the newly-serializable event types) and 23
      `totem-console` tests (13 new: 6 view-model parse tests, 7 view
      SSR-render tests).
- [x] tests:integration — 15 new `totem-store` integration tests
      (`tests/governance.rs`'s 8, `tests/access_log.rs`'s 2 new cases) and
      10 new `totem-gateway` integration tests
      (`tests/promotion_uncertainty_audit.rs`), all over the embedded
      `kv-mem` engine.
- [ ] ci:passed — not claimed. CI runs on `advance/**` pushes, so a real
      pipeline result exists for this branch once pushed, but this run did
      not read it back.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CONSOLE-002 --status passed`
- Checks run externally this session: `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`
  (all green — see the journey report for the exact commands and a note on
  an environment disk-space constraint that required a clean rebuild
  mid-session), plus `cargo build`/`cargo clippy --target
  wasm32-unknown-unknown` for the console crate specifically, since its
  `api.rs` only compiles on that target.

## Changes Made

### 2026-08-06 - tidy: Serialize/Deserialize on promotion and curation events
- `crates/totem-core/src/promotion.rs`: `PromotionEventKind`, `PromotionEvent`
  derive `Serialize`/`Deserialize`; one round-trip test.
- `crates/totem-core/src/curation.rs`: `CurationEventKind`, `Supersession`,
  `CurationEvent` derive `Serialize`/`Deserialize`; one round-trip test built
  from a real two-original merge.

### 2026-08-06 - test/feat: core: Governance::resolve + AccessOperation variants
- `crates/totem-core/src/record.rs`: new `GovernanceError`
  (`NotADecision`/`NotPending`) and `Governance::resolve` — only from
  `Pending`, only to `Approved`/`Rejected`; three tests.
- `crates/totem-core/src/access_log.rs`: `AccessOperation` gains `Propose`,
  `PromotionDecision`, `Resolve`.
- `crates/totem-core/src/lib.rs`: exports `GovernanceError`.

### 2026-08-06 - test/feat: store: pending_review, resolve_review, access-log for_memory
- `crates/totem-store/tests/governance.rs`: new — 8 tests (queue
  scope/ordering, resolution clearing the queue, the double-decision and
  not-pending refusals, the not-found refusals).
- `crates/totem-store/tests/access_log.rs`: 2 new tests (`for_memory`'s
  scope check and ordering).
- `crates/totem-store/src/memory.rs`: `MemoryRepository::pending_review` and
  `resolve_review`.
- `crates/totem-store/src/access_log.rs`: `AccessLogRepository::for_memory`
  (visibility-checked via `MemoryRepository::get`); operation key mappings
  for the three new `AccessOperation` variants.
- `crates/totem-store/src/row.rs`: `review_key` widened to `pub(crate)` for
  reuse outside `row.rs`'s own `to_row`/`from_row`.
- `crates/totem-store/src/error.rs`: `StoreError::Governance`,
  `StoreError::ReviewDecided`.
- `crates/totem-store/src/schema.rs`, `migrate.rs`: migration 7, widening
  `access_log.operation`'s assertion for `propose`/`promotion_decision`/
  `resolve` (the same `OVERWRITE` technique migration 4 used).

### 2026-08-06 - test/feat: gateway: promotion approval, uncertainty, and audit REST surface
- `crates/totem-gateway/tests/promotion_uncertainty_audit.rs`: new — 10
  integration tests.
- `crates/totem-gateway/src/dto.rs`: request/response DTOs for all six new
  endpoints (`ProposePromotionRequest`/`Response`,
  `PromotionQueueRequest`/`Response`, `ProposedRecordRequest`/`Response`,
  `PromotionDecisionRequest`/`Response`,
  `UncertaintyQueueRequest`/`Response`,
  `ResolveUncertaintyRequest`/`Response`, `AuditRequest`/`AuditTrailResponse`).
- `crates/totem-gateway/src/ops.rs`: `propose_promotion`, `promotion_pending`,
  `proposed_record`, `approve_promotion`/`reject_promotion`,
  `pending_uncertainty`, `resolve_uncertainty`, `audit_trail`.
- `crates/totem-gateway/src/handlers.rs`: the six REST handlers.
- `crates/totem-gateway/src/lib.rs`: routes `POST /promotions`,
  `POST /promotions/pending`, `POST /promotions/:id/record`,
  `POST /promotions/:id/approve`, `POST /promotions/:id/reject`,
  `POST /uncertainty/pending`, `POST /uncertainty/:id/resolve`,
  `POST /audit/:id`; exports the new DTOs.
- `crates/totem-gateway/src/error.rs`: status mappings for
  `StoreError::Promotion*`/`Curation*`/`Governance`/`ReviewDecided` — these
  previously fell through to a generic 500 because no surface had
  exercised them yet.

### 2026-08-06 - feat: console: view models, views, and wiring
- `crates/totem-console/src/view_model.rs`: `parse_promotion_queue`,
  `parse_proposed_record`, `parse_uncertainty_queue`,
  `AuditTrailViewModel`/`parse_audit_trail`; 6 tests. Reuses
  `totem_core::PromotionEvent`/`CurationEvent`/`AccessLogEntry` directly now
  that they derive `Deserialize`.
- `crates/totem-console/src/app.rs`: `PromotionQueueView`,
  `UncertaintyQueueView`, `AuditTrailView`; two new tabs (Governance,
  Audit) on `App`; 7 SSR tests using a harness-component pattern for the
  `EventHandler` props Dioxus can only construct inside a live component.
- `crates/totem-console/src/api.rs`: `fetch_promotion_queue`,
  `approve_promotion`/`reject_promotion`, `fetch_uncertainty_queue`,
  `resolve_uncertainty`, `fetch_audit_trail` behind a shared `post_json`
  helper; `RootApp` wires them into Refresh and a separate audit-lookup
  form. Untested by unit test (wasm32-only network calls), verified to
  build and lint clean for `wasm32-unknown-unknown`.

## Check for Understanding

1. `ops::contest` (ADV-GATEWAY-004) deliberately never sets the contested
   record's `governance.status` to `Contested` — it only files a new
   Uncertainty record. Given that, what field on the *new* Uncertainty
   record is actually responsible for it appearing in
   `MemoryRepository::pending_review`'s queue, and why does that follow
   automatically from `Governance::initial` rather than needing new code in
   `ops::contest`?

2. `MemoryRepository::resolve_review` (`crates/totem-store/src/memory.rs`)
   both calls `record.governance.resolve(decision)` (a pure, in-memory
   check against the record it already fetched) and repeats
   `governance.review = $pending` as a `WHERE` predicate on the actual
   `UPDATE`. What failure would disappear if the `WHERE` predicate were
   dropped and only the pre-check remained, and why is
   `StoreError::ReviewDecided` a distinct error from
   `StoreError::Governance(GovernanceError::NotPending(_))` rather than the
   same one?

3. `AccessLogRepository::for_memory` calls `MemoryRepository::new(self.db).get(reader, id)`
   itself, even though every caller (`ops::audit_trail`) already calls
   `memories().get()` first to build the record in the response. Given
   that redundancy, what concrete request could reach `for_memory` without
   going through `ops::audit_trail`'s own check, and why does the security
   guidance in `docs/cloud-agent-notes.md` treat that redundancy as
   required rather than wasteful?

4. `totem-core`'s `PromotionEvent` and `CurationEvent` needed
   `Serialize`/`Deserialize` added in this advance's first commit before
   anything else could be built. Why couldn't `totem-gateway`'s `dto.rs`
   just define its own parallel `PromotionEventDto`/`CurationEventDto`
   structs instead, the way it does for none of its other wire types, and
   what would that have cost `crates/totem-console/src/view_model.rs`
   specifically?

5. `PromotionQueueView` and `UncertaintyQueueView`
   (`crates/totem-console/src/app.rs`) both bind `let id = proposal.id;` (or
   `record.id`) to a local variable *before* the `button { onclick: move |_| ... }`
   closures that use it, rather than referencing `proposal.id`/`record.id`
   directly inside the closure. What Rust error does removing that
   temporary produce, and why does it only show up inside a `for` loop over
   `.iter()` rather than when rendering a single fixed item?

6. The advance's own Objective named a fourth deliverable — value/usage
   reports and a "retire?" queue — that this run did not build. Where in
   this file is that decision recorded, and what would a future advance
   implementing it need to add to `totem-store`'s `MemoryRepository` that
   `pending_review` does *not* already provide?

7. `decide_promotion` in `crates/totem-gateway/src/ops.rs` does not call
   `caller.authorize_scope` the way `save`/`propose_promotion` do. Given
   that an approver's authority to decide a proposal is instead enforced
   inside `PromotionRepository`'s own `open_proposal` (checking the
   *decider's* chain against the proposal's `to_scope`), what would a
   `caller.authorize_scope` call on this function need to check against —
   and why is there no single `Scope` value available at this call site to
   check it against before the store call happens?
