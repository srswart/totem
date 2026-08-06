---
advance:
  id: "ADV-GATEWAY-003"
  title: "Streamable-HTTP MCP + auth for cloud agents"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T12:56:54Z"
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 91
  risk_flags: ["auth", "public_api", "new_dependency", "concurrency"]
  evidence: ["tidy:preparatory", "tdd:red-green", "tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

Serve MCP over streamable HTTP with token auth so cloud agents (Cursor
background agents, Anthropic cloud sessions, CI-driven agents) can attach
(Solution Intent §3.1). Tokens are least-privilege: bound to repo + scope
(gateway invariant). Verify per-harness remote-MCP support at build time
(open question in §9).

Follows [Tech Direction: MCP](../../../../docs/tech-direction/mcp.md):

- **MCP-001** — the `StreamableHttpService`-on-`axum::Router` mount shape this
  advance uses was already executed end-to-end by that spike; `rmcp` stays
  pinned `=3.1.0`.
- **MCP-003 / MCP-004** — streamable HTTP is what the two verified cloud
  harness classes prefer or require, and the Claude API connector is
  tools-only, which Totem's surface already is.
- **MCP-005** — the Cursor capability gap that spike left open was re-checked
  here and is **still open**; see Behavioral Change below.

## Behavioral Change

After this advance:

- The gateway has two compositions, and only one is meant to face a network.
  `totem_gateway::router` is the local one (in-process tests, single-user
  desktop): every caller is `Caller::Trusted` and identity is taken at its
  word, unchanged from before. `totem_gateway::authenticated_app` is the
  remote one: the same REST routes **plus** the MCP tool surface at `POST
  /mcp` over streamable HTTP, every route behind bearer-credential
  verification.
- A cloud agent holding a scoped credential calls the same seven tools desktop
  harnesses get (`totem_recall`, `totem_save`, `totem_landscape`,
  `totem_feedback`, `totem_contest`, `totem_advance_log`,
  `totem_advance_status`) over streamable HTTP — verified by a real `rmcp`
  client against a real loopback listener, not by calling the tool functions
  directly.
- A credential bound to repo A + one scope + one actor **cannot**: act as
  another actor, name another repo, claim team membership it was not issued
  for, widen an `actor:`-bound chain to project scope, or write the `platform`
  scope. Each refusal has its own test, and the whole set was re-run against a
  deliberately neutered authorization to confirm the tests detect a broken
  implementation (see Evidence).
- Issuance and revocation exist as a library API (`TokenRegistry::issue`,
  `register`, `revoke`, `verify`) plus a single environment-seeded bootstrap
  credential for the binary. There is deliberately **no unauthenticated HTTP
  endpoint that mints credentials** — console/CLI wiring lands separately, as
  this advance's own scope note anticipated.
- The registry stores SHA-256 fingerprints, never token text, so a state dump
  is not a credential dump.
- An empty registry refuses every request. A gateway that served callers
  whenever it had no credentials configured would fail open exactly when it is
  least configured.

**MCP-005 is carried forward as an explicitly unverified assumption, not
closed.** The tech direction asked this advance to either re-run the Cursor
capability check from a reachable environment or accept it as unverified.
`docs.cursor.com` was re-attempted here and still fails with
`CONNECT tunnel failed, response 403` — the same egress block. Totem's
streamable-HTTP surface is therefore designed against the two *verified*
harness classes (Claude Code, the Claude API MCP connector); Cursor
background-agent reach remains sourced from secondary material only and needs
a Cursor session or a reachable environment to confirm.

## Planned Implementation Tasks

- [x] branch: create or confirm feature branch for this advance
- [x] tidy: preparatory refactoring (no behavior change)
- [x] test: authn/authz tests — token scope bounds, expiry, revocation (red first)
- [x] feat: streamable-HTTP MCP transport + token auth layer

## Bug Fixes

- [ ] None. One test assertion of this advance's own was wrong and was
  corrected before the implementation commit: it assumed `RepoId::new` rejects
  a value that is not `owner/name`-shaped. `totem-core` validates only
  empty/untrimmed ids, by design. Isolation does not rest on that shape — it
  rests on *equality* between the bound id and the one a request asserts —
  so the assertion was rewritten to test what `totem-core` actually promises.

## Risk + Rollback

- Risk: auth flaw here is a scope-leak vector (the highest-severity failure
  class); treat authorization tests as blocking evidence.
- Rollback: disable the HTTP MCP listener (config), leaving stdio unaffected;
  revoke issued tokens.
- Residual risk, stated plainly: `router()` still builds an unauthenticated
  composition. It is the right thing for in-process tests and a loopback
  desktop deployment, and it is what every pre-existing test uses — but
  binding *it* to a public listener bypasses everything this advance adds.
  That is prevented by naming and documentation, not by the type system.
- Residual risk: credentials are process-local and non-persistent, matching
  the in-memory store. A restart forgets every credential, and a
  multi-instance deployment does not share them. Durable credential storage
  belongs with the durable deployment (ADV-INFRA-001), which is the plan item
  immediately before this one and is `WORKSTATION`-designated.

## Evidence

- [x] tidy:preparatory — `tidy: [ADV-GATEWAY-003] extract AppState::in_memory()`
      landed first and changed no behavior; the gateway's six pre-existing test
      targets passed unchanged across it.
- [x] tdd:red-green — the test commit precedes the implementation commit. Red
      was captured against the committed tree with the implementation stashed:

      error[E0432]: unresolved import `totem_gateway::TokenRegistry`
      error[E0425]: cannot find function `authenticated_app` in crate `totem_gateway`
      error[E0609]: no field `tokens` on type `AppState`

- [x] tests:unit — 11 passing in `src/auth.rs` and `src/error.rs`.
- [x] tests:integration — 17 passing in `tests/auth.rs`, including four real
      `rmcp`-client-over-loopback-HTTP round trips.
- **Mutation check (stronger than red-green, and the reason to trust the
  above):** with `Caller::authorize_identity` and `Caller::authorize_scope`
  stubbed to always return `Ok` for a bound caller, **8 of the 17 integration
  tests failed** — exactly the eight authorization cases — while the
  authentication and happy-path cases still passed. The tests detect a broken
  implementation rather than merely passing against a working one.
- `ci:passed` is **not** claimed: no pipeline result was read from this run.

### One workspace test target is red, and it is not this advance's

`cargo test --workspace` is **not** fully green. `totem-arrive-sync`'s
`tests/dogfood.rs` fails two of its three tests:

```
  left: ["arrive-sync", "cli", "console", "core", "curator", "gateway", "infra", "store"]
 right: ["arrive-sync", "cli", "console", "core", "curator", "gateway", "store"]

  left: 8
 right: 7
```

The repo has eight components; the test still expects the seven that existed
before `infra` was added (commit `d661bb5`, "pin DEP-001 deployment topology
and author ADV-INFRA-001"). **This reproduces exactly at `origin/master`** —
verified by checking `crates/` and `arrive/` out at master and re-running the
target — so it predates this branch and is unrelated to it: this advance adds
no component and touches no component artifact.

Deliberately **not** fixed here. The fix is a one-line expectation update in
the `arrive-sync` component, and folding it into a security advance for the
`gateway` component would widen this advance's declared footprint to hide an
unrelated breakage inside it. It wants its own change. Flagged so whoever
reviews this sub-PR knows the red target is inherited, not introduced.

## What this advance establishes only partially

Stated plainly, because `gateway.yaml`'s components are stage `incubating` and
the CLI does not enforce their invariants:

1. **`/enroll` and `GET /landscape/:repo` are authenticated but not
   repo-bound.** Any valid credential can sync or read any repo's landscape.
   The credential's `repo` is a memory-scope id (`owner/name`); a landscape is
   keyed by the ARRIVE registry id (`058-totem`). These are different id
   spaces — `mcp.rs`'s `LandscapeParams` doc already says so — and binding one
   to the other needs a reconciliation this advance does not attempt. The
   landscape is not scoped memory, so this is not a memory-isolation leak, but
   it is a real gap in "least-privilege".
2. **Refused requests append nothing to the access log.** No read or write
   goes unlogged, because a refused request never reaches the store and
   touches no memory. But *attempts* are not recorded either, and an
   audit trail of rejected credentials is worth having. Recording them needs a
   new `AccessOperation` variant in `totem-core` — the `core` component, out
   of this advance's declared scope.
3. **Expiry is wall-clock, with no skew tolerance.** `Utc::now()` at
   verification time; a credential expires the instant its timestamp passes.
4. **`allowed_hosts` is left at `rmcp`'s loopback-only default.** Correct for
   these tests; a public deployment must widen it to its own hostnames, which
   belongs with ADV-INFRA-001's deployment rather than hard-coded here.
5. **Team-scoped credentials are enforced but not exercised end to end over
   HTTP.** The grant matrix for `Scope::Team` has unit coverage and one
   integration test for the refusal path; no test issues a team-bound
   credential and reads a team-scoped memory through it.

## Reviewability

`arrive score` reports **91 [RED]** (size 65, novelty 11, risk 15; flags
`auth`, `concurrency`). The budget says split at Red or document why the change
is atomic. **Documented, not split** — and here is the case, so a reviewer can
disagree with it rather than just accept it.

**Why it does not split into two shippable halves.** The two obvious cut lines
both produce something worse than the whole:

- *Transport first, credentials later* leaves a remote MCP surface reachable
  with no credential on it. That is precisely the failure mode the project
  brief names as highest-severity, shipped deliberately and left in the tree
  until a follow-up lands.
- *Credentials first, transport later* leaves an authorization layer with
  nothing remote to guard. Its invariant could then only be tested against the
  in-process router — and the specific thing this advance must prove is that a
  *cloud* agent's calls are bounded, over the transport a cloud agent actually
  uses. The end-to-end tests that make the invariant credible would not exist
  yet.

The tech direction points the same way: MCP-005 asked that the token design and
the streamable-HTTP surface be settled together, not in sequence.

**What the 64-point size actually is.** 1,240 inserted lines, of which:

- **651 are `tests/auth.rs`** — over half the diff, and the part a reviewer of
  a security change should want *more* of, not less.
- **227 are this advance record.**
- **~520 are production code**, concentrated in two new files (`auth.rs`,
  `mcp_http.rs`). `auth.rs` is itself about 40% unit tests.
- The remaining eight source files are shallow mechanical edits: threading
  `caller` through five `ops` functions, five REST handlers, and seven MCP
  tools. That breadth is the point — it is what makes "a new surface forgot to
  check the credential" fail to compile rather than fail silently — but no
  individual site needs more than a glance.

**Suggested review order**, since the diff is wide but not deep:
`src/auth.rs` (the whole model lives there) → `src/lib.rs` (the two
compositions, and why neither can run without a `Caller`) → `tests/auth.rs`
(what is actually proven, and the mutation-check note in Evidence) →
everything else, which is mechanical.

## CI Evidence Notes

- If CI jobs are temporarily disabled, run checks externally before merge:
  - `arrive pr check --strict --json`
  - `arrive evidence record --advance ADV-GATEWAY-003 --status passed`

## Changes Made

### 2026-08-06 - tidy: Extract AppState::in_memory()

- crates/totem-gateway/src/state.rs: added `AppState::in_memory()`, the
  store + migrate + embedder construction all three call sites had spelled out
- crates/totem-gateway/src/main.rs: calls it instead of building state inline
- crates/totem-gateway/src/bin/mcp_stdio.rs: same
- crates/totem-gateway/tests/common/mod.rs: same, taking the store handle back
  off the state

### 2026-08-06 - test: Cloud-agent credential bounds over both surfaces

- crates/totem-gateway/tests/auth.rs: 17 tests — authentication (missing,
  unknown, revoked, expired, fingerprint-only storage), authorization (other
  actor, other repo, unclaimed team, actor-bound widening, platform writes,
  over-scoped issuance), and four streamable-HTTP round trips driven by a real
  `rmcp` client against a real loopback listener
- crates/totem-gateway/Cargo.toml: `transport-streamable-http-server` on the
  `rmcp` dependency and `transport-streamable-http-client-reqwest` on the dev
  dependency, both at the existing `=3.1.0` pin; `sha2` for fingerprints and
  `uuid` for credential material

### 2026-08-06 - feat: Streamable-HTTP MCP behind bearer credentials

- crates/totem-gateway/src/auth.rs: new. `TokenGrant` (repo + scope + actor +
  expiry), `AuthError` split into 401 and 403 cases, `Caller`
  (`Trusted` | `Bound`), `TokenRegistry` (fingerprint-keyed issue / register /
  revoke / verify), and the `authenticate` axum middleware
- crates/totem-gateway/src/mcp_http.rs: new. Mounts the MCP tool surface at
  `/mcp` over streamable HTTP, building only a token-bound handler
- crates/totem-gateway/src/lib.rs: split the route table out of `router()`, so
  `router()` (trusted, local) and `authenticated_app()` (credential-bound,
  remote) each attach exactly one caller and neither can be reached without one
- crates/totem-gateway/src/ops.rs: every operation now takes the `Caller` and
  authorizes before touching the store — the single enforcement point both
  surfaces pass through
- crates/totem-gateway/src/handlers.rs: REST handlers extract
  `Extension<Caller>` and hand it to `ops`
- crates/totem-gateway/src/mcp.rs: `TotemMcp` gained a token-bound mode; every
  tool reads the verified caller out of the request extensions `rmcp` injects,
  and refuses when the mode requires one and none is there
- crates/totem-gateway/src/error.rs: `GatewayError::Auth`, mapped to 401 (with
  `WWW-Authenticate: Bearer`) or 403 by whether it is an authentication failure
- crates/totem-gateway/src/state.rs: `AppState::tokens`
- crates/totem-gateway/src/main.rs: serves `authenticated_app`, seeds one
  bootstrap credential from the environment, warns loudly when none is set
- crates/totem-cli/tests/enroll.rs: builds its gateway through
  `AppState::in_memory()` rather than an `AppState` literal, which the new
  `tokens` field would otherwise have broken. Found by
  `cargo clippy --workspace --all-targets`, not by the gateway's own tests —
  a cross-crate call site the per-package run does not reach.

### 2026-08-06 - docs: Complete advance record

- arrive/systems/058-totem-core/advances/ADV-GATEWAY-003.md: status
  `complete`, `implementation_completed_at`, evidence actually produced,
  reviewability score and its justification, the partially-established
  invariants, and a fresh Check for Understanding
- arrive/implementation-plan.yaml: ADV-GATEWAY-003 `planned` → `done`

## Check for Understanding

1. `auth.rs` says this module "does not filter a single result", yet
   `TokenGrant::authorize_scope` refuses a `platform` write that the store
   would have accepted. Why is that not a contradiction — and what property of
   `ScopeChain::resolve` makes the `platform` case the one place a credential
   check adds a restriction the chain cannot express?

2. `ops::save` takes `caller: &Caller` as a parameter rather than reading a
   thread-local or looking the credential up itself. What class of future
   mistake does that signature prevent, and why would a helper function that
   *both* surfaces are expected to call not have prevented it?

3. `TotemMcp::new` yields a `Trusted` handler and `TotemMcp::token_bound` a
   credential-bound one; only `mcp_http::routes` constructs the latter. If a
   future advance mounts the MCP surface on a new HTTP route and forgets the
   `authenticate` layer, what does the resulting server do on a tool call, and
   which line of `mcp.rs` decides that?

4. `an_actor_bound_token_cannot_widen_its_chain_to_project_scope` asserts a
   `403` for a request naming `project: srswart/totem` and a `200` for the
   same request naming no project — with the *same* credential, whose `repo`
   field is `srswart/totem`. Why is naming its own repo a refusal?

5. The mutation check neutered both `Caller` authorization methods and 8 of 17
   integration tests failed. Nine still passed. Name two of those nine and
   explain why a broken authorization layer does not affect them — and what
   that says about which tests are load-bearing for this advance's invariant.

6. `AuthError::UnknownCredential` is returned for both a forged token and a
   revoked one, and `verify` cannot tell them apart. What would be leaked by
   distinguishing them, and where in `TokenRegistry::revoke` is that decision
   actually made?

7. The advance records that `/enroll` is authenticated but not repo-bound.
   Which two identifier spaces would have to be reconciled to close that gap,
   and which existing doc comment in `mcp.rs` already explains why they are
   not interchangeable?
