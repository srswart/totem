---
advance:
  id: "ADV-CORE-003"
  title: "Scope promotion policy engine (gap-fill)"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T15:00:00Z"
  review_time_estimate_minutes: 145
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 82
  risk_flags: ["auth", "migration", "public_api"]
  evidence: ["tidy:preparatory", "tdd:red-green", "tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

**Gap-fill** (see docs/arrive-decomposition-gaps.md): "sharing is by
promotion" is a pillar of the model (Solution Intent §2.2), and ADV-CONSOLE-002
builds the approval UI — but no advance builds the promotion mechanics.
Implement propose/approve/reject/demote as recorded events with per-category
policy: auto-approval for low-risk categories (e.g. Knowledge), human-gated
for Instructions.

## Behavioral Change

After this advance:

- A memory at `actor` scope can be proposed for `project`/`team`/`platform`
  scope. The proposal, the decision, and the effective scope change are rows in
  an append-only `promotion_event` table, each carrying full `Provenance`.
- Policy decides the path per category, and it reads the category's *existing*
  `ReviewPolicy` rather than a second table: Knowledge/Identity/Context
  auto-promote, Instructions/Uncertainty queue for a human. Episodic memory is
  `Forbidden` — an episodic row cannot be touched at all, so moving one is
  impossible rather than merely gated.
- `PromotionRepository::pending` is the queue ADV-CONSOLE-002 renders, filtered
  to proposals aimed at a scope the reader can reach.
- Demotion exists as the compensating event and is never gated: narrowing
  reduces exposure, so it must not queue behind the gate that let the promotion
  through.
- No promotion is a silent in-place scope edit. `MemoryRepository` still never
  writes `scope` after `save`; the only statement in the workspace that does
  runs inside the transaction that records the event authorising it, and that
  transaction throws unless exactly one row moved.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change)
- [x] test: policy-path tests per category, incl. rejection + demotion (red first)
- [x] feat: promotion events, policy engine, store enforcement

## Bug Fixes

- [ ] None found. No defect in existing code surfaced during this advance.

## Risk + Rollback

- Risk: promotion is the one sanctioned path across scope boundaries — a
  policy bug is a scope-leak vector (the highest-severity failure class).
- Rollback: demotion events compensate any bad promotion; policy can be
  tightened to human-gated-for-everything via config.

Both halves of the rollback are now executable rather than aspirational:
`PromotionRepository::demote` is available to anyone who could have promoted,
and `Store::promotions_with_policy(PromotionPolicy::human_gated_everywhere())`
puts every category behind a human without editing a category definition. Each
has a test that fails if the lever stops working.

## Security Notes

The three authorization rules this engine enforces, and where each is checked:

1. **You may only propose what you can already read.** The record is fetched
   through `MemoryRepository::get` with the proposer's own chain, so a record
   outside it reads as absent (`StoreError::NotFound`), never as forbidden.
2. **You may only propose into a scope you can already reach.** The target must
   be in the proposer's chain — the same rule `save` applies to a new record,
   applied to the destination of a move. A decision is held to the same rule
   against the *decider's* chain, and a proposal aimed outside it reads as
   absent (`StoreError::PromotionNotFound`).
3. **The move and its event are one transaction.** `UPDATE` matching no rows is
   not an error in SurrealQL, so the statement throws unless exactly one row
   moved. There is no recorded promotion that did not happen, and no promotion
   that happened without a record.

**One deliberate disclosure, stated plainly.** `proposed_record` is the single
place a reader sees a record its own chain does not reach. This is what
proposing *means*: asking the target scope's reviewers to read a private note
so they can decide on it. The disclosure is bounded on every side — the
proposal must still be open, the reviewer must be able to reach the scope it
targets, and the record fetched is pinned to the id and origin scope the
proposal itself recorded, so no other row is reachable through it. It returns
`None` the moment the proposal is decided. Without it a reviewer would be
approving blind, which is a worse security posture, not a better one.

**Approval crosses a boundary the approver's chain does not cover.** Moving
ada's record out of `actor:ada` is authorised by the proposal ada wrote, not by
the approver's own reach. The effecting `UPDATE` therefore pins on
`scope = $from` (the origin the proposal recorded) rather than the approver's
chain, which would otherwise make approval impossible. This is the intended
model, not an oversight, and it is why propose-time checking is strict.

**Ordering is store-assigned.** `provenance.created_at` is whatever the calling
harness reported, so the trail is ordered by a `recorded_at` the store stamps.
A caller with a wrong — or deliberately backdated — clock cannot rearrange the
record of who decided what.

### Guards verified by mutation

Each of these was removed in turn and the suite re-run, to check the tests fail
for the right reason rather than passing against a broken implementation:

| Guard removed | Tests that failed |
| --- | --- |
| `to_scope IN $scopes` in `open_proposal` | `nobody_can_decide_a_proposal_aimed_at_a_scope_they_cannot_reach`, `a_reviewer_reads_the_record_a_pending_proposal_names_and_nothing_else` |
| the proposer's target-reachability check | `nobody_can_propose_into_a_scope_they_cannot_reach` |
| the already-decided check | `a_decided_proposal_cannot_be_decided_again`, `a_reviewer_reads_...` |
| `IF array::len($moved) != 1 { THROW ... }` | `an_event_is_never_recorded_for_a_move_that_did_not_happen`, `the_transaction_refuses_an_episodic_move_the_policy_would_have_caught_first` |

### Invariants established only partially

- **Access log.** The security guidance for this phase asks that every path
  touching memory append to the access log. Promotion does not, and this is a
  disclosed residual rather than an oversight: the store writes no access-log
  entries anywhere today — `MemoryRepository::save` and `recall` do not either,
  because ADV-GATEWAY-001 put that write at the surface that handles the
  request. Promotion has no surface yet. When one exposes it (ADV-CONSOLE-002,
  or a gateway advance), it must append an entry, and that needs a new
  `AccessOperation` variant plus a migration widening the `operation` assertion
  — the same shape as migration 4's `feedback`. The promotion trail is itself
  append-only with full provenance, so promotion is auditable now; what is
  missing is the *who-asked-when* record of the request that triggered it.
- **`governance.review` is untouched by promotion.** An Instructions record
  starts at `ReviewState::Pending` for its own content review, and this advance
  deliberately leaves that field alone: approving a promotion is a decision
  about *where the record lives*, not about whether its content is trusted.
  Conflating the two would let a promotion approval silently discharge a
  content review nobody performed. If the console wants one action to do both,
  that is a product decision to make explicitly.
- **Concurrency.** Two reviewers approving the same proposal at the same instant
  are not serialised by a lock; the already-decided check reads before it
  writes. The blast radius is bounded — the second `apply` finds `scope = $from`
  no longer true and throws, so the outcome is one move and one spurious error
  rather than two moves — but a genuinely serialised decision would need a
  uniqueness constraint on `proposal`. Recorded here rather than claimed.

## Reviewability

`arrive score` reports **82 [RED]** (size 64, novelty 8, risk 10; risk flag:
`migration`). This is documented as atomic rather than split.

Splitting at the component boundary — core policy in one sub-PR, store
enforcement in the next — would ship a `PromotionPolicy` that nothing obeys,
sitting on the phase branch until the follow-up lands. For the one advance in
this phase whose failure mode is a silent scope leak, an interval where the
policy exists and the enforcement does not is exactly the wrong artifact to
put in front of a reviewer. The plan also carries this as a single item, so a
split would need a new planned advance, which is a human scoping decision
rather than one for an agent run.

What the number is actually made of: of ~2,190 added lines, roughly 1,150 are
tests — the two new test files, the in-crate transaction-guard module, and four
new schema assertions. The production surface a reviewer must reason about is
`crates/totem-core/src/promotion.rs` (343 lines, over a third of it doc
comments) and `crates/totem-store/src/promotion.rs` (~480 lines before its test
module). The `## Security Notes` table above is offered as the reading order:
each guard, and the test that proves it.

## Evidence

- [x] tidy:preparatory — two tidy commits, no behavior change, full suite green
      between them
- [x] tdd:red-green — core tests failed to compile (no `PromotionPolicy`), the
      three new `promotion_event` schema tests failed against a missing table,
      and the store integration tests failed to compile, all before any
      implementation landed
- [x] tests:unit — `cargo test --workspace`: 265 passed, 0 failed
- [x] tests:integration — 15 store integration tests over the embedded `kv-mem`
      engine; 10 core policy tests
- [ ] ci:passed — not claimed. CI runs on `advance/**` pushes, so a real
      pipeline result exists for this branch, but this run did not read it.

Two tests here are honestly *not* red-green pairs and are labelled as such:
`the_database_refuses_a_scope_edit_on_an_episodic_row` is a standing regression
guard that already passed against migration 1, and the two transaction-guard
tests in `promotion.rs` were written alongside the statement they cover — their
evidence is the mutation run in the table above, not authoring order.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CORE-003 --status passed`

## Changes Made

### 2026-08-06 - tidy: Extract uuid_id! macro from MemoryId
- crates/totem-core/src/ids.rs: `MemoryId`'s hand-written derives, `Display`,
  and `FromStr` became a `uuid_id!` macro, so a second machine-minted id does
  not duplicate them. Generated impls are identical to those replaced.

### 2026-08-06 - tidy: Share the row helpers a second table needs
- crates/totem-store/src/row.rs: provenance mapping lifted out of
  `to_row`/`from_row` into `provenance_to_row`/`provenance_from_row`;
  `readable_scopes` and `objects` moved in from `memory.rs`, so the scope
  predicate has one definition rather than one per repository.
- crates/totem-store/src/memory.rs: uses the shared helpers; the two local
  functions and three now-unused imports removed. Same fields, spellings, and
  error strings.

### 2026-08-06 - test: Promotion policy paths, scope enforcement, recorded events
- crates/totem-core/tests/promotion.rs: 10 policy tests — per-category paths,
  the widening and narrowing rules, the tightened policy, and the constructor
  rules that stop a decision naming a different record than its proposal.
- crates/totem-store/tests/promotion.rs: 15 integration tests — auto-promotion,
  the human-gated queue, rejection, double-decision, the three refusals, the
  scope-filtered queue and history, the bounded reviewer disclosure, demotion
  as rollback, and provenance on every event.
- crates/totem-store/tests/common/mod.rs: `decided_by` fixture.
- crates/totem-store/src/schema.rs: three `promotion_event` schema tests
  (append-only `UPDATE`/`DELETE`, provenance required) plus a regression guard
  that the database refuses a scope edit on an episodic row.

### 2026-08-06 - feat: Scope promotion policy engine
- crates/totem-core/src/promotion.rs: new — `PromotionPolicy` (per-category
  path from the category's own `ReviewPolicy`; `human_gated_everywhere` as the
  tightening lever), `PromotionPath`, `PromotionError`, `PromotionEventKind`,
  and `PromotionEvent` with constructors that make a mismatched decision
  unexpressible.
- crates/totem-core/src/ids.rs: `PromotionId`.
- crates/totem-core/src/lib.rs: module and exports.
- crates/totem-store/src/promotion.rs: new — `PromotionRepository`
  (`propose`, `approve`, `reject`, `demote`, `pending`, `history`,
  `proposed_record`), `PromotionOutcome`, the row mapping, and the
  single-transaction `apply` with its row-count `THROW`. In-crate tests cover
  that guard, which the public API cannot reach.
- crates/totem-store/src/schema.rs: `PROMOTION_SCHEMA_V5` — append-only
  `promotion_event` with required provenance and a store-assigned
  `recorded_at`.
- crates/totem-store/src/migrate.rs: migration 5, `scope_promotion_events`.
- crates/totem-store/src/store.rs: `promotions()` and
  `promotions_with_policy()`.
- crates/totem-store/src/error.rs: `Promotion`, `PromotionNotFound`,
  `PromotionDecided`.
- crates/totem-store/src/lib.rs: module and exports.

## Check for Understanding

1. `PromotionPolicy::path` does not carry its own per-category table — it reads
   `category.lifecycle().review`. What failure would a second, independent table
   have made possible, and which test in `crates/totem-core/tests/promotion.rs`
   would stop catching it if the two were allowed to diverge?

2. Episodic memory is `PromotionPath::Forbidden` rather than
   `PromotionPath::HumanGated`. Given that migration 1's `EVENT` already refuses
   `UPDATE` on an episodic row, why does `crates/totem-core/src/promotion.rs`
   refuse it a second time in Rust, and what does the store's
   `the_transaction_refuses_an_episodic_move_the_policy_would_have_caught_first`
   prove that the core test cannot?

3. `apply` in `crates/totem-store/src/promotion.rs` pins its `UPDATE` on
   `scope = $from` rather than on `scope IN $scopes` the way every other
   statement in the crate does. Whose authority moves the record in that
   statement, and what would break if the approver's chain were used instead?

4. `proposed_record` returns a record the caller's own `ScopeChain` cannot
   reach. Name the four conditions that bound that disclosure, and explain why
   removing it would make the system *less* safe rather than more.

5. `promotion_event` carries both `provenance.created_at` and a store-assigned
   `recorded_at`, and `pending`/`history` order by the latter. What attack does
   ordering by the former enable, and why is it not enough to say "callers
   should send honest timestamps"?

6. `UPDATE` matching zero rows is not an error in SurrealQL. Trace what
   `an_event_is_never_recorded_for_a_move_that_did_not_happen` would observe if
   the `IF array::len($moved) != 1 { THROW ... }` line were deleted from
   `apply` — what exactly ends up in `promotion_event`, and why is that worse
   than an error?

7. Demotion is never gated, while promotion of the same category may be. State
   the asymmetry in one sentence, and say what `demotion_cannot_hand_a_record_to_another_actor`
   is guarding against — since demotion is supposedly the safe direction.

8. The advance records that promotion writes no access-log entry. Why is that a
   consistent choice given where `MemoryRepository::save` logs today, and what
   two changes must land together when a surface first exposes promotion?
