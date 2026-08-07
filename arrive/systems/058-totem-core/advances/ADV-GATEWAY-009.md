---
advance:
  id: "ADV-GATEWAY-009"
  title: "Repo-bind enroll and landscape reads; unify the repo id spaces"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "arrive-sync", "cli"]
  started_at: "2026-08-06T13:45:00Z"
  implementation_completed_at: "2026-08-07T03:40:00Z"
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 55
  risk_flags: ["auth", "migration", "concurrency"]
  evidence: ["tidy:preparatory", "tdd:red-green", "tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: complete
---

## Objective

Close the enumeration vector ADV-GATEWAY-003 disclosed as a partially
established invariant: `/enroll` and `GET /landscape/:repo` are authenticated
but not repo-bound, so any valid credential can enroll or read the landscape
of **any** repo. Bind both routes to the credential's repo, which first
requires unifying the two repo id spaces that made the binding impossible to
express (`TokenGrant.repo` is an `owner/name` `RepoId`; the landscape sync
uses ARRIVE landscape ids like `058-totem`).

This must land before any second repo enrolls (the reserved
ADV-ARRIVE-SYNC-002 multi-repo sync), and before ADV-GATEWAY-006 evaluates
scope isolation — the evaluation must test the bound behavior, not grade the
hole (wired as a plan dependency).

## Behavioral Change

After this advance:

- Decided (correcting this advance's original either/or framing): the
  landscape keeps its existing ARRIVE-id primary key (`repo.id`, e.g.
  `"058-totem"`) — the console and this repo's own dogfood tests read it
  unchanged, and neither is in this advance's declared components — and
  additionally carries the `owner/name` id (`RepoArtifact.git_repo` /
  `RepoView.git_repo`), the same id space `TokenGrant.repo` already speaks.
  `registry.yaml` gains a `git_repo` field alongside `repo_id` as the source
  of truth. A credential's binding is checked against `git_repo`, never
  against the ARRIVE id.
- `POST /enroll` (REST and any MCP equivalent) refuses a snapshot whose repo
  identity is not the presenting credential's repo, with an `AuthError` that
  names both — and separately refuses a snapshot that would *rebind* an
  ARRIVE id another repo already owns, even when the submitted `git_repo`
  matches the caller's own binding (see Bug Fixes: this second check was
  missing at first merge and landed in a follow-up fix).
- `GET /landscape/:repo` and the `totem_landscape` MCP tool refuse a repo the
  credential is not bound to. No route can enumerate other repos' landscapes.
- Negative tests prove both refusals, plus a control proving the bound repo
  still round-trips; the ADV-GATEWAY-003 auth suite keeps passing unchanged.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] test: repo-mismatch refusals for enroll and landscape (REST + MCP),
      bound-repo control, id-space round-trip
- [x] feat: id-space unification + repo binding on both routes
- [x] tidy: `cargo fmt` (no behavior change), landed after test/feat since the
      inline `TokenGrant`/handler bodies didn't stabilize until the refusal
      logic itself was written

## Bug Fixes

- [x] **Enroll rebind / landscape hijack (found by Copilot review on the
      merged PR #43, fixed in a follow-up sub-PR since #43 had already
      merged before the review comment landed).** `sync()` upserts by the
      snapshot's ARRIVE id and unconditionally overwrites `git_repo`. The
      original `enroll` handler only checked that the *submitted* snapshot
      named the caller's own repo — it never checked whether the ARRIVE id
      being enrolled already belonged to a *different* repo. A credential
      bound to repo B could therefore take over an ARRIVE id already owned by
      repo A simply by asserting `git_repo: B` in the snapshot, rewriting A's
      landscape out from under it — a hijack, not merely an unauthorized
      read, and strictly worse than the enumeration vector this advance set
      out to close. Fixed: `enroll` now looks up any existing row for the
      named ARRIVE id and, for a `Caller::Bound` credential, refuses unless
      that row is unowned (first claim) or its `git_repo` already matches the
      caller's own binding — an existing row with no confirmed `git_repo` yet
      is refused the same way an unconfirmed landscape *read* already was.
      Proven by
      `enrolling_a_snapshot_cannot_rebind_an_arrive_id_another_repo_already_owns`,
      which also asserts the rightful owner can still read its landscape
      afterward and can still re-sync it.

## Scope and Boundaries

**In scope:** the two unbound routes, the id-space unification they require,
and the migration of any existing landscape rows to the unified id.

**Out of scope:** auth-refusal logging (ADV-CORE-006); multi-repo sync
itself (reserved ADV-ARRIVE-SYNC-002); console human auth (see gaps doc,
now suggested as ADV-GATEWAY-010).

## Risk + Rollback

- Risk (`auth` flag): the binding must refuse — not filter. A landscape
  response that silently omits unauthorized repos would hide the violation
  from the caller and the audit trail alike; refuse loudly with the bound
  and requested ids, reusing the existing `AuthError::RepoNotBound` (this
  advance's original text named a non-existent `AuthError::RepoMismatch` —
  corrected here; `RepoNotBound` was already the variant `authorize_identity`
  raises for a project-scope mismatch, and it fits this refusal unchanged).
- Risk (`migration` flag): id-space unification touches stored landscape
  rows. Resolved additively rather than by rekeying: `repo` keeps its
  existing ARRIVE-id primary key, and gains an `option<string> git_repo`
  field (schema migration 8, `repo_git_identity`) that a pre-unification row
  simply lacks until its next sync converges it — `LandscapeRepository::sync`
  already `UPSERT CONTENT`s the whole row, so no bespoke backfill statement
  was needed. Exercised by a unit test that seeds a raw pre-migration `repo`
  row (no `git_repo`) directly against the connection, then asserts a normal
  `sync()` call converges it.
- Risk (`concurrency` flag, `arrive score`-detected): no new concurrency
  primitive was added; the flag reflects the pre-existing `RwLock` in
  `auth.rs` this advance's new `TokenGrant`/`Caller` methods sit alongside.
- Rollback: revert branch; the routes return to authenticated-but-unbound,
  which is the previously recorded residual, not a regression. The added
  `git_repo` schema field and column are additive and harmless to leave in
  place even if the routes' binding checks are reverted.

## Evidence

- [x] `tdd:red-green` — the `test:` commit (`08ba311`) precedes the `feat:`
      commit (`8608403`). Red was a compile failure, the same convention
      ADV-GATEWAY-003 used: at the test commit, `RepoArtifact`/`RepoView`'s
      `git_repo` field the new tests and fixtures reference does not exist
      yet, so `cargo build`/`cargo test` fail across every crate that
      constructs a `RepoArtifact` literal or an enroll JSON fixture. The
      `feat:` commit makes the workspace compile and every new assertion
      pass.
- [x] `tidy:preparatory` — a separate `tidy: cargo fmt` commit (`179eb17`)
      landed after test/feat rather than before: `cargo fmt --check` only
      flagged formatting once the refusal logic's exact line lengths existed,
      so there was nothing to tidy in advance of writing it. No behavior
      changed in that commit.
- [x] `tests:unit` — `crates/totem-gateway/src/auth.rs`'s
      `a_grant_permits_its_own_repo_and_refuses_another` /
      `a_trusted_caller_has_no_repo_binding_to_check`;
      `crates/totem-store/src/landscape.rs`'s
      `a_repo_synced_before_id_unification_gains_git_repo_on_the_next_sync`
      (seeds a raw pre-migration `repo` row via the crate-private connection,
      proving the migration story without a bespoke backfill statement).
- [x] `tests:integration` — `crates/totem-gateway/tests/auth.rs` gained 6
      tests over the real composed `authenticated_app`/`router` applications:
      `enrolling_a_snapshot_naming_another_repo_is_refused`,
      `enrolling_a_snapshot_for_the_bound_repo_succeeds`,
      `reading_another_repo_s_landscape_is_refused`,
      `reading_the_bound_repo_s_landscape_succeeds`,
      `a_bound_token_cannot_confirm_a_never_synced_repo_s_binding`, and the
      MCP-surface twin
      `totem_landscape_refuses_a_repo_this_token_cannot_confirm_over_streamable_http`.
      `crates/totem-store/tests/landscape_sync.rs` and the existing
      `crates/totem-gateway/tests/{enroll,landscape,feedback_contest_advance}.rs`
      suites (unchanged behavior, updated fixtures) all pass.

## CI Evidence Notes

- No hosted CI pipeline is configured for this repo yet — `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace` were run directly in this sandbox (Step 7); their
  results are reported plainly in the sub-PR, not inferred from a pipeline
  run that did not happen.

## Changes Made

### 2026-08-07 - test: repo-mismatch refusals for enroll and landscape (REST + MCP)
- crates/totem-gateway/tests/auth.rs: new `get`/`enroll_body` helpers and 6
  repo-binding tests (cross-repo refusal, bound-repo control, and the
  unconfirmed/never-synced refusal, over REST and MCP).
- crates/totem-gateway/tests/enroll.rs, landscape.rs,
  feedback_contest_advance.rs: fixtures gain `git_repo`.
- crates/totem-store/tests/landscape_sync.rs: fixture gains `git_repo`; the
  sync test now asserts it round-trips.

### 2026-08-07 - feat: unify repo id spaces and bind enroll/landscape to the credential
- arrive/registry.yaml: new `git_repo` field (`srswart/totem`), the
  `owner/name` identity alongside the existing ARRIVE `repo_id`.
- crates/totem-arrive-sync/src/lib.rs: `RegistryFields`/`read_registry`
  carry `git_repo` into `RepoArtifact`.
- crates/totem-store/src/landscape.rs: `RepoArtifact`/`RepoView` gain
  `git_repo`; `sync()` writes it, `view()` reads it back; new unit test
  proves a pre-migration row converges on re-sync.
- crates/totem-store/src/schema.rs, migrate.rs: migration 8
  (`repo_git_identity`) adds `option<string> git_repo` to the `repo` table.
- crates/totem-gateway/src/auth.rs: `TokenGrant::authorize_repo` /
  `Caller::authorize_repo`, plus unit tests.
- crates/totem-gateway/src/handlers.rs: `enroll`/`landscape` extract
  `Extension<Caller>` and call `authorize_repo` before syncing/returning.
- crates/totem-gateway/src/mcp.rs: `totem_landscape` does the same repo-bind
  check the REST handler does, resolving the landscape's own `git_repo`
  (falling back to the raw path id when unsynced).

### 2026-08-07 - tidy: cargo fmt
- crates/totem-gateway/src/handlers.rs, tests/auth.rs,
  crates/totem-store/src/landscape.rs: formatting only.

### 2026-08-07 - test/fix: enroll rebind hijack (Bug Fixes)
- crates/totem-gateway/tests/auth.rs: new
  `enrolling_a_snapshot_cannot_rebind_an_arrive_id_another_repo_already_owns`
  test, confirmed to fail (compiles, but the hijack succeeds with `200 OK`
  instead of `403`) before the fix.
- crates/totem-gateway/src/handlers.rs: `enroll` now looks up the existing
  landscape row for the snapshot's ARRIVE id and, for `Caller::Bound`,
  refuses to sync unless that row is unowned or its `git_repo` already
  matches the caller's binding.

## Check for Understanding

1. Why does `GET /landscape/:repo` compare a `Caller::Bound` credential
   against `RepoView::git_repo` instead of against the `:repo` path segment
   directly? (See `handlers::landscape`'s comment and the Objective: `:repo`
   is the ARRIVE registry id, a different id space than `TokenGrant.repo`.)
2. `a_bound_token_cannot_confirm_a_never_synced_repo_s_binding` expects a
   `403`, not a `404` or an empty `200`. Why does an unconfirmed binding have
   to look identical to a real mismatch, and what would go wrong if a
   never-synced repo returned `200` with an empty landscape for a bound
   caller the way it still does for a trusted one?
3. `RepoArtifact::git_repo` is required (`String`); `RepoView::git_repo` is
   `Option<String>`. Why the asymmetry, and what does `None` mean on read
   that can never happen on write?
4. `crates/totem-store/src/schema.rs` adds `REPO_GIT_IDENTITY_SCHEMA_V8`
   rather than editing `MEMORY_SCHEMA_V1`'s existing `DEFINE TABLE repo`
   statement. What would go wrong for an already-migrated deployment if the
   original migration were edited in place instead?
5. `TokenGrant::authorize_repo` and `Caller::authorize_repo` are two methods
   doing what looks like the same comparison. What does `Caller::Trusted`'s
   branch buy that inlining `TokenGrant::authorize_repo` everywhere would
   lose?
6. `enroll`'s rebind check (Bug Fixes) only runs for `Caller::Bound`, never
   for `Caller::Trusted`. Why is that the right split, given a Trusted caller
   already skips every other `authorize_*` check in this codebase — and what
   would a `Caller::Trusted` deployment need to add if it ever stopped being
   single-user?
7. The rebind check treats an existing row with `git_repo: None` (a
   pre-migration row nobody has re-synced) as *not* matching any credential's
   binding, refusing even the repo that "should" own it. Why is refusing the
   safer default here, and what has to happen before that repo's own
   credential can enroll into it again?
