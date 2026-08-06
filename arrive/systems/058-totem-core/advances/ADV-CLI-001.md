---
advance:
  id: "ADV-CLI-001"
  title: "totem CLI: enroll, sync hook install, actor credentials (gap-fill)"
  system: "058-totem-core"
  primary_component: "cli"
  components: ["cli", "gateway", "arrive-sync", "store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T06:27:28Z"
  review_time_estimate_minutes: 40
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth", "public_api", "new_dependency"]
  evidence:
    - "tidy:preparatory"
    - "tdd:red-green"
    - "tests:unit"
    - "tests:integration"
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

**Scope correction, made during implementation:** the objective as originally
written implies a *verified* least-privilege credential — but ADV-GATEWAY-003
("Streamable-HTTP MCP + auth for cloud agents", `MODEL: claude-opus-5`,
security-critical per CLAUDE.md), which owns the gateway's token-verification
design, is still `status: planned` and Opus-gated; this Sonnet-only run cannot
implement it. `totem credential create` therefore issues the *shape* of a
least-privilege credential — validated, repo+scope+actor-bound, stored
locally with restrictive permissions — and states plainly that the gateway
does not check any token on any request yet (unchanged by this advance: every
`/save`/`/recall`/`/enroll` call already accepted a caller-asserted identity
before and after). This is recorded here rather than silently narrowing scope,
per CLAUDE.md's "Honesty rules" and "Security-critical advances" guidance.

## Behavioral Change

After this advance:
- `totem enroll --repo-root <path> --gateway-url <url>` parses the repo's
  `/arrive/` tree locally (reusing `totem-arrive-sync::read_repo_artifacts`)
  and POSTs the resulting landscape snapshot to a new gateway endpoint,
  `POST /enroll` (`totem-gateway`), which calls
  `totem-store`'s `LandscapeRepository::sync` directly — registering the repo
  and running the first ingestion in one call. Unless `--no-hook` is passed,
  it also installs a `post-commit` git hook that re-invokes `totem enroll`, so
  subsequent `/arrive/` changes sync automatically — the coverage success
  measure ("% of advances visible within minutes of change") becomes
  reachable. The hook install refuses to overwrite a `post-commit` hook it did
  not itself install, and re-installing (e.g. after a gateway URL change) is
  idempotent.
- `totem credential create --repo <owner/name> --scope <scope> --actor <id>`
  issues an opaque token bound to exactly one repo, one scope, and one actor —
  validated with `totem-core`'s own `RepoId`/`ActorId`/`Scope` parsers, and
  refused if the scope names a different repo (`project:...`) or a different
  actor (`actor:...`) than requested. Stored at `~/.totem/credentials.json`
  (or `$TOTEM_HOME`), `0600` on unix. See the Scope correction above and Risk
  + Rollback below for what this credential does not yet guarantee.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change) — `Serialize`/
      `Deserialize` derives on `totem-store`'s landscape artifact types, so a
      `LandscapeSnapshot` can cross the CLI→gateway process boundary as JSON
- [x] test: enroll flow tests against a test gateway (red first)
- [x] feat: `totem enroll`, hook installer, credential commands

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: credential handling on developer machines (storage, leakage) —
  mitigated by `0600` permissions on the credential store file (unix) and by
  scope/repo/actor validation refusing an internally-inconsistent credential
  before it is minted. **Residual, stated plainly:** the token is not yet
  verified by the gateway — nothing server-side rejects a forged or
  unbound-scope token today, because no request-authentication path exists at
  all yet. That verification is ADV-GATEWAY-003's job; until it lands, this
  credential is a local artifact with the right shape, not an enforced
  security boundary. Tracked so ADV-GATEWAY-006 (the security evaluation)
  does not assume otherwise.
- Risk: the sync hook must be safe to run in any repo state — mitigated by
  refusing to overwrite a `post-commit` hook this crate did not install
  (`HookError::ForeignHookExists`), and by requiring `.git/hooks/` to exist
  before writing (`HookError::NotAGitRepo`).
- Risk: `POST /enroll` has no scope chain to resolve (a landscape sync is not
  scoped memory, unlike `/save`/`/recall`) and, per the credential risk above,
  no caller authentication yet — any caller who can reach the gateway can
  enroll or re-sync any repo's landscape. This is the same posture `/save` and
  `/recall` already have (caller-asserted identity, no verification), not a
  regression this advance introduces; ADV-GATEWAY-003 closes it for all three
  endpoints together.
- Rollback: `totem unenroll` does not exist yet (not required by the
  Behavioral Change above — re-syncing is idempotent and additive, and a
  landscape mirror is disposable/re-derivable from `/arrive/` per
  `arrive-sync.yaml`'s own invariant); remove the installed hook file by hand
  or `git config core.hooksPath` around it. A credential is revoked by
  deleting its entry from `~/.totem/credentials.json` — there is no
  server-side revocation surface yet, consistent with there being no
  server-side verification yet.

## Evidence

- [x] tidy:preparatory — `tidy:` commit adds `Serialize`/`Deserialize` to
      `totem-store`'s `RepoArtifact`/`SystemArtifact`/`OwnerArtifact`/
      `ComponentArtifact`/`AdvanceArtifact`/`LandscapeSnapshot`, derive-only,
      no field or logic change; `cargo test -p totem-store` (35 tests across
      `scope_isolation`, `schema_contracts`, `lifecycle`, `landscape_sync`,
      plus the crate's own unit/doc tests) passes unchanged.
- [x] tdd:red-green — confirmed by execution, following the same
      `unimplemented!()`-scaffold pattern ADV-GATEWAY-001 used:
      - `test:` commit adds the full `totem-cli` crate scaffold (all public
        types: `Credential`, `CredentialError`, `HookError`,
        `EnrollSummary`, `EnrollError`) and the gateway's `POST /enroll`
        route/DTOs, with every function body `unimplemented!("ADV-CLI-001")`.
        `cargo test -p totem-cli -p totem-gateway --no-fail-fast`: all 4 new
        test targets fail — `totem-cli`'s `credential` (9/9), `enroll`
        (3/3), and `hook` (4/4) tests panic with `not implemented:
        ADV-CLI-001`; `totem-gateway`'s `enroll` test binary fails 2/3 the
        same way, with the third
        (`a_malformed_enroll_body_never_reaches_the_store`) passing because
        Axum's `Json` extractor rejects a request missing a required field
        before any handler runs — the same "malformed input never reaches
        the handler" behavior the pre-existing `/save`/`/recall` tests rely
        on. Pre-existing gateway suites (`mcp_recall_and_save`,
        `recall_and_save`) stayed green throughout.
      - `feat:` commit replaces every stub with real logic, no signature
        changes. `cargo test --workspace --no-fail-fast`: full green,
        including the 4 previously-red targets.
- [x] tests:unit — `totem-cli::credential` (9 tests: issuance binds
      repo/scope/actor, tokens are unique, project/actor scope-repo mismatch
      is refused, invalid scope/actor is refused, store/load round-trips and
      appends, an absent store file is an empty list, the store file is
      `0600` on unix); `totem-cli::hook` (4 tests: install writes an
      executable hook naming the gateway, re-install is idempotent and
      updates the URL, a non-git directory is refused, a foreign hook is
      never overwritten).
- [x] tests:integration — `totem-cli::enroll` (3 tests, against a real
      bound-socket gateway — `totem-cli` is a separate process from
      `totem-gateway` in practice, so this drives the actual HTTP client path
      rather than an in-process `tower::oneshot` call): enrolling this repo's
      real `/arrive/` tree populates the gateway's landscape (systems == 1,
      advances >= 23, matching `totem-arrive-sync`'s own dogfood fixture
      count); an unreachable gateway reports plainly
      (`EnrollError::Request`); a missing `/arrive/` directory reports
      plainly (`EnrollError::Ingest`). `totem-gateway::enroll` (3 tests):
      enrolling syncs the landscape and reports a summary, re-enrolling the
      same repo is idempotent (no duplicate rows, one `sync_run` per call),
      a malformed body never reaches the store.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-CLI-001 --status passed`

## Changes Made

### 2026-08-06 - tidy: Serialize/Deserialize on landscape artifact types
- `crates/totem-store/src/landscape.rs`: added `Serialize`/`Deserialize` to
  `RepoArtifact`, `SystemArtifact`, `OwnerArtifact`, `ComponentArtifact`,
  `AdvanceArtifact`, `LandscapeSnapshot` — derive-only, no behavior change
- `crates/totem-store/Cargo.toml`: updated the `serde` dependency comment to
  reflect the new write-side (not just read-side/view-type) use

### 2026-08-06 - test: totem-cli scaffold, gateway /enroll route, and tests (red)
- `Cargo.toml`: added `crates/totem-cli` to workspace members
- `crates/totem-cli/`: new crate (`totem` binary) — `Cargo.toml`; `src/lib.rs`
  (`home_dir`, module wiring); `src/credential.rs` (`Credential`,
  `CredentialError`, `issue`/`load`/`store`/`default_store_path` signatures,
  bodies `unimplemented!`); `src/hook.rs` (`HookError`, `install` signature,
  body `unimplemented!`); `src/enroll.rs` (`EnrollSummary`, `EnrollError`,
  `enroll` signature, body `unimplemented!`); `src/main.rs` (clap CLI:
  `enroll`, `credential create`)
- `crates/totem-cli/tests/{credential,hook,enroll}.rs`: new test files
- `crates/totem-gateway/src/dto.rs`: added `EnrollRequest` (flattens
  `totem_store::LandscapeSnapshot`), `EnrollResponse`
- `crates/totem-gateway/src/handlers.rs`: added `enroll` handler, body
  `unimplemented!`
- `crates/totem-gateway/src/lib.rs`: wired `POST /enroll` into the router;
  re-exported the new DTOs
- `crates/totem-gateway/tests/enroll.rs`: new test file

### 2026-08-06 - feat: enroll, sync hook install, credential issuance
- `crates/totem-cli/src/credential.rs`: `issue` validates repo/scope/actor via
  `totem-core`, refuses a scope/repo or scope/actor mismatch, mints a 256-bit
  opaque token; `load`/`store` implement the local JSON credential store with
  `0600` permissions on unix
- `crates/totem-cli/src/hook.rs`: `install` writes the `post-commit` script,
  refusing to overwrite a foreign hook, setting `0755` on unix
- `crates/totem-cli/src/enroll.rs`: `enroll` parses `/arrive/` via
  `totem-arrive-sync`, POSTs the snapshot (plus `source`) to
  `<gateway_url>/enroll`, and decodes the summary
- `crates/totem-gateway/src/handlers.rs`: `enroll` calls
  `state.store.landscape().sync(&request.snapshot, &request.source)`

## Check for Understanding

1. `totem credential create` issues a token even though nothing in the
   gateway verifies it today. What specifically would a malicious caller
   still be able to do that a "least-privilege, repo+scope-bound" credential
   is supposed to prevent, and which advance closes that gap? (See the Scope
   correction in Objective and the first Risk + Rollback bullet.)
2. `crates/totem-cli/src/credential.rs::issue` refuses `--repo srswart/totem
   --scope project:other/repo --actor ada` but allows `--repo srswart/totem
   --scope team:058-totem --actor ada`. Why does the mismatch check apply to
   `Scope::Project` and `Scope::Actor` but not `Scope::Team`/`Scope::Platform`?
3. `POST /enroll` (`crates/totem-gateway/src/handlers.rs`) calls
   `state.store.landscape().sync(...)` directly, with no `ScopeChain` in
   sight — unlike `/save` and `/recall`, which both resolve one via
   `ScopeChain::resolve` before touching the store. Why is a scope chain not
   applicable here, and what governs who is allowed to call `/enroll` today?
4. `crates/totem-cli/src/hook.rs::install` reads the existing hook file and
   checks for a marker string before deciding whether to overwrite it. Walk
   through what happens on each of: no `post-commit` file, a `post-commit`
   file this function wrote before, and a `post-commit` file a developer
   wrote by hand — which `HookError` variant (if any) does each produce?
5. The `test:` commit stubs every new function with
   `unimplemented!("ADV-CLI-001")` rather than deleting the crate/route
   entirely. What does `cargo test -p totem-cli -p totem-gateway` report for
   `a_malformed_enroll_body_never_reaches_the_store` in that red state, and
   why does it pass even though `handlers::enroll`'s body never runs?
