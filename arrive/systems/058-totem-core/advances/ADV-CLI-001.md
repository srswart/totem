---
advance:
  id: "ADV-CLI-001"
  title: "totem CLI: enroll, sync hook install, actor credentials (gap-fill)"
  system: "058-totem-core"
  primary_component: "cli"
  components: ["cli", "gateway", "arrive-sync"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T06:17:56Z"
  review_time_estimate_minutes: 178
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 98
  risk_flags: ["auth", "concurrency"]
  evidence: ["tdd:red-green", "tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

**Gap-fill** (see docs/arrive-decomposition-gaps.md): enrollment is a core flow
(Solution Intent §3.3) and the `cli` component exists in the decomposition, but
no roadmap advance builds it. Implement `totem enroll` (register repo, trigger
initial ARRIVE ingestion, install the sync hook) and actor enrollment
(obtain a scoped credential).

**Correction to the original objective:** "register repo" against "the
gateway" reads as a distinct HTTP call, but no such endpoint exists —
`totem-gateway` exposes only `/recall` and `/save`, and has no auth layer at
all yet (`SaveRequest`/`RecallRequest` trust a caller-supplied `author` as
provenance; ADV-GATEWAY-003, still `planned`, owns building real
authentication). `totem-gateway`'s own binary also has no durable deployment
today — it stands up a fresh embedded in-memory `Store` on every start,
because deployment topology is itself an open question
(docs/solution-intent.md §9). Given that, "register" is implemented as: the
first successful landscape sync *is* the registration event — there is no
separate persisted "repo record" to create beyond what
`LandscapeRepository::sync` already writes (the `repo` graph entity,
ADV-ARRIVE-SYNC-001). `totem enroll` and `totem sync` each connect their own
throwaway embedded store, matching `totem-gateway`'s own current approach
exactly, so upgrading to a durable connection later is a change to *where*
these functions connect, not to the ingestion logic itself.

## Behavioral Change

After this advance:
- `totem enroll [--path PATH]` runs the repo's first landscape sync (via the
  real `totem_arrive_sync::sync_repo` / `totem_store::Store` API
  ADV-ARRIVE-SYNC-001 built) and installs a git hook into
  `.git/hooks/post-commit` and `.git/hooks/post-merge` — idempotent,
  marker-guarded, mirrors `hooks/platform/install.sh`'s pattern. The hook body
  itself is embedded in the `totem` binary and materialized into the enrolled
  repo's own `.git/hooks/` at enroll time, because (unlike this repo's
  platform hooks) the repo being enrolled does not have a `hooks/` source tree
  of its own to point at.
- `totem sync [--path PATH]` re-runs the sync alone — what the installed hook
  calls on every commit/merge, and what a person can run by hand.
- **Known limitation, stated plainly:** because there is no durable store
  connection yet (§9 open question), state does not persist *between* CLI
  invocations — each `enroll`/`sync` call populates and then discards its own
  in-memory store. The coverage success measure ("% of advances visible
  within minutes of change") is not fully reachable until a durable store
  lands; this advance proves the ingestion-triggering mechanics (sync +
  hook), not persistence, which is `docs/arrive-decomposition-gaps.md`'s
  documented `ADV-INFRA-001` gap.
- `totem credential issue --repo <repo> --scope <scope> --actor <actor>`
  mints and locally persists a scoped credential (`repo` + `scope` bound, per
  the `gateway` component's stated invariant) to
  `$TOTEM_HOME/credentials.json` (default `$HOME/.totem`), permissioned
  `0600`/`0700` on unix. `totem credential list` and
  `totem credential revoke --id <id>` manage what was issued.
- **Known limitation, stated plainly:** no gateway endpoint verifies these
  credentials yet — ADV-GATEWAY-003 owns server-side token verification and
  revocation. This advance builds the local half only: an actor can obtain
  and revoke a scoped credential, but nothing today checks it at the door.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change) — none needed; this
      advance adds a new crate rather than refactoring existing code
- [x] test: enroll/sync/hook/credential tests (red first)
- [x] feat: `totem enroll`, `totem sync`, hook installer, credential commands

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: credential handling on developer machines (storage, leakage).
  Mitigated by `0600`/`0700` file permissions on unix (verified by
  `tests/credential.rs::the_credential_file_is_not_group_or_world_readable`)
  and by keeping secrets out of the store's list/print paths except at
  issuance. Not mitigated: non-unix permissions (no ACL restriction applied
  on Windows — this workspace's CI and every contributor machine today are
  unix, so this is recorded as a real gap rather than silently scoped out),
  and there is no encryption at rest.
- Risk: the hook must be safe to run in any repo state. Mitigated by
  `set -u` (not `-e`) in the hook body, silent no-ops when `totem` is not on
  `PATH` or `/arrive/` is absent, and an env-var kill switch
  (`TOTEM_SYNC_HOOK=0`) — the same degrade-gracefully conventions
  `hooks/platform/arrive-platform-pre-commit.sh` already uses in this repo.
- Rollback: remove the `# totem-sync-hook`-marked lines from
  `.git/hooks/post-commit` / `.git/hooks/post-merge` and delete
  `.git/hooks/totem-sync-hook` (no `totem unenroll` command exists yet — a
  real gap, not built here); `totem credential revoke --id <id>` removes a
  locally-issued credential from the local registry. Because no gateway
  verifies credentials yet, "revoke" today means "stop honouring it locally"
  — there is nothing server-side to revoke against until ADV-GATEWAY-003
  lands.

## Reviewability

`arrive score --base origin/advance/phase-005` reports **98, RED** (63 size /
20 novelty / 15 risk). Splitting further was considered and rejected:

- The advance's own decomposition (`docs/arrive-decomposition-gaps.md`)
  already scopes `ADV-CLI-001` as one gap-fill advance covering both
  enrollment and actor credentials — creating a second advance id to split
  them is a re-plan this run is not mandated to make.
- The credential store (`credential.rs`) is functionally independent of
  enroll/hook and could theoretically land alone, but doing so would leave
  the advance's stated Objective ("`totem enroll`... and actor enrollment")
  half-delivered across two PRs with no natural boundary in the plan to
  attach the second half to.
- What actually drives the size is one new crate from nothing (`Cargo.toml`,
  5 source modules, 3 test files, a checked-in hook-body script) — the same
  "wholly new module/crate" shape `ADV-ARRIVE-SYNC-001` scored 100/RED for
  and did not split further, for the same reason: the pieces verify only as
  one behavior (`tests/enroll.rs` exercises sync + hook together).
- The `concurrency` flag is `arrive score`'s pattern match on
  dependency-manifest churn (`Cargo.lock`, new-crate `Cargo.toml`) and the
  presence of `#[tokio::main]`/`async fn`, not a concurrency-bearing code
  change — no threading, shared mutable state, or async coordination logic
  was added (every `async fn` here does one sequential `await` chain: connect
  → migrate → sync). Recorded as detected rather than suppressed, matching
  `ADV-ARRIVE-SYNC-001`'s precedent for this same flag.

## Evidence

- [x] tidy:preparatory — not applicable; no preparatory refactor was needed
- [x] tdd:red-green — genuine for the whole advance, not partial: all three
      test files (`tests/hook_install.rs`, `tests/credential.rs`,
      `tests/enroll.rs`) were written and committed before any module they
      exercise existed. `cargo test -p totem-cli` at that commit failed to
      compile ("can't find bin `totem`"; `credential`/`enroll`/`error`/`hook`
      modules declared in `lib.rs` with no backing files) — the same
      wholly-new-module red convention `ADV-ARRIVE-SYNC-001` used
      (`0259e35`). The following `feat:` commit made all 10 tests pass
      without editing any test file.
- [x] tests:unit — none isolated at the unit level; every behavior here only
      means something through its filesystem/process/store side effects, so
      it is covered as integration tests below rather than duplicated as
      unit tests of pure logic that doesn't exist independently.
- [x] tests:integration — `tests/hook_install.rs` (3 tests: fresh install
      creates both hook files, a repeated install is byte-for-byte
      idempotent, installing into a pre-existing hook file preserves its
      content); `tests/credential.rs` (4 tests: issuing returns unique
      ids/secrets and both round-trip through `list()`, revoking removes an
      entry and a second revoke of the same id errors rather than
      no-op'ing, an issued credential's repo/scope/actor round-trip exactly,
      the credential file is `0600` on unix); `tests/enroll.rs` (3 tests:
      `enroll` against a synthetic `/arrive/` tree in a temp git repo syncs
      the landscape *and* installs both hooks, `sync` alone syncs without
      touching hooks, enrolling a nonexistent path fails with
      `CliError::NotAGitRepo`). All 10 pass; `cargo clippy -p totem-cli
      --all-targets -- -D warnings` and `cargo fmt -p totem-cli` are clean.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CLI-001 --status passed`

## Changes Made

### 2026-08-06 - test: enroll/sync/hook/credential tests (red first)

- `Cargo.toml`: registered `crates/totem-cli` as a workspace member; removed
  its now-obsolete placeholder-slot comment line.
- `crates/totem-cli/Cargo.toml`: new — first use of `clap` (derive feature)
  anywhere in this workspace, and first use of `tempfile` (dev-dependency,
  integration tests only).
- `crates/totem-cli/hooks/totem-sync-hook.sh`: new — the hook body script,
  checked in so its shell logic is reviewable as text rather than only as a
  Rust string literal; embedded into the binary via `include_str!`.
- `crates/totem-cli/src/lib.rs`: new — declares the `credential`/`enroll`/
  `error`/`hook` modules the tests below already call, none of which existed
  yet (the intended red state).
- `crates/totem-cli/tests/hook_install.rs`, `tests/credential.rs`,
  `tests/enroll.rs`: new — see Evidence for what each covers.

### 2026-08-06 - feat: totem enroll/sync + local scoped credentials

- `crates/totem-cli/src/error.rs`: new — `CliError`, wrapping I/O, JSON,
  git-repo-resolution, and the existing `IngestError`/`StoreError` types.
- `crates/totem-cli/src/hook.rs`: new — `install()`, the idempotent
  marker-guarded git hook installer; materializes the embedded hook-body
  script into the target repo's `.git/hooks/` and appends a `$(dirname "$0")`
  invocation line into `post-commit` and `post-merge`.
- `crates/totem-cli/src/credential.rs`: new — `Credential`, `CredentialStore`
  (`issue`/`list`/`revoke` against `$TOTEM_HOME/credentials.json`, `0600`/
  `0700` permissions on unix via `std::os::unix::fs::PermissionsExt`).
- `crates/totem-cli/src/enroll.rs`: new — `enroll()`/`sync()`, both resolving
  the git worktree root via `git rev-parse --show-toplevel` before connecting
  a throwaway embedded `totem_store::Store` and calling
  `totem_arrive_sync::sync_repo`.
- `crates/totem-cli/src/main.rs`: new — the `totem` binary; `clap` derive
  `enroll` / `sync` / `credential {issue,list,revoke}` subcommands dispatching
  to the functions above.

## Check for Understanding

1. `totem enroll`'s doc comment says "the first successful sync *is* the
   registration event." Why does this advance not create a separate
   persisted "repo record," and what would have to change (see the
   Objective's Correction) before that stopped being true?
2. `hook::install` writes the hook *body* into the target repo's own
   `.git/hooks/totem-sync-hook` instead of referencing a path inside this
   repo's `hooks/` tree the way `hooks/platform/install.sh` does. Why does
   that difference matter here specifically — what's different about who
   `totem enroll` runs against versus who `hooks/platform/install.sh` runs
   against?
3. `tests/hook_install.rs::installing_into_an_existing_hook_file_appends_without_clobbering_it`
   seeds a hook file with a shebang and one `echo` line before installing.
   What would break for a real user if `install_invocation` overwrote the
   file instead of appending to it?
4. `credential.rs`'s doc comment says revocation today means "stop honouring
   it locally," not a server-side revoke. Trace why: what would a
   `totem credential revoke` need to talk to before it could actually deny a
   previously-issued credential, and which planned advance owns building
   that?
5. `enroll.rs` and `totem-gateway/src/main.rs` both call
   `Store::in_memory()` fresh on every run. What specific behavior would a
   caller incorrectly assume works today if they didn't know that, and
   where in this advance's Behavioral Change is that limitation stated?
