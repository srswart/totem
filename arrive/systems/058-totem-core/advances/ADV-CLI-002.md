---
advance:
  id: "ADV-CLI-002"
  title: "CLI authenticates to the gateway"
  system: "058-totem-core"
  primary_component: "cli"
  components: ["cli", "gateway"]
  started_at: "2026-08-07T15:30:00Z"
  implementation_completed_at: "2026-08-07T21:00:00Z"
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 49
  risk_flags: ["auth"]
  evidence: ["tests:unit", "tdd:red-green", "enroll:executed"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: complete
---

## Objective

`totem enroll` and `totem sync` send **no credential** — `crates/totem-cli/src/enroll.rs`
contains no authorization header at all. They worked in every demo because
the gateway was an unauthenticated loopback composition; against the deployed
gateway (`https://totem-dev.fly.dev`) every CLI call returns 401. This is the
third instance of the same gap the deployment exposed — MCP connectors
(ADV-GATEWAY-011/013), the console (ADV-GATEWAY-010), and now the CLI: every
client was built against a gateway that never refused anything.

The CLI is how a repo gets enrolled and how the post-commit hook keeps the
landscape fresh, so without this the dogfood trial has no landscape.

## Behavioral Change

After this advance:

- Every CLI call to the gateway carries a bearer credential, resolved in a
  documented precedence: `--token` flag, then `TOTEM_TOKEN` in the
  environment, then the **local credential store** ADV-CLI-001 already
  writes at `~/.totem/credentials.json`, matched on the repo and gateway
  being addressed.
- Missing or rejected credentials produce an actionable error naming the
  precedence and how to obtain one — not a bare `401` or a stack trace. A
  developer meeting this for the first time must be told what to do.
- `--gateway-url` (and the credential it implies) is persistable, so the
  post-commit sync hook installed by `totem enroll` works unattended without
  the token appearing in the hook script or in `git` — the hook reads the
  stored configuration, and the advance records exactly where the token
  lives and with what file permissions.
- `totem credential` gains the ability to obtain a credential *from the
  gateway* (ADV-GATEWAY-012's issuance endpoint) rather than only minting a
  local record with nothing behind it, and stores it in the same place the
  other commands read.
- The token is never logged, never printed on success, and never written to
  a file mode broader than `0600` — asserted by test, since ADV-CLI-001
  already established the permission convention.

## Bootstrapping note

Issuance is itself an authenticated operation, so the first credential on a
machine comes from outside the CLI: the deployment's bootstrap credential
(`fly secrets`), pasted once via `--token` or `TOTEM_TOKEN`. The advance
should say this plainly in its error text and runbook rather than leaving a
new user to deduce it.

## Planned Implementation Tasks

- [x] branch / claim
- [x] tidy: none needed — `enroll` had one call site and one request builder
- [x] test: precedence honoured; a credential for another repo is not used;
      absence fails with a message naming the whole order; a resolved
      credential never renders its token
- [x] feat: resolution, bearer on the request, `--token`/`--repo` flags,
      credential-free hook

## A second defect, found only by calling the deployed gateway

`totem-cli`'s `reqwest` was configured `default-features = false` with only
`json` — **no TLS backend at all**. Every `https://` URL was refused before a
connection was attempted:

    invalid URL, scheme is not http

So the CLI could not have reached *any* real deployment, credential or not.
No unit test would have caught it: the enroll tests spawn a local HTTP
listener, which is precisely the shape that hides this. It was found by
pointing the built binary at `https://totem-dev.fly.dev` — the same lesson as
the MCP schema bug and the unauthenticated clients before it, arriving for
the third time: **the contract that matters is the one an external caller
exercises.**

Worth noting for whoever bumps versions: the feature is `rustls` on reqwest
0.13 (which the CLI pins) and `rustls-tls` on 0.12 (which the gateway uses),
so the two crates' manifests differ for a real reason rather than by
oversight.

## Scope and Boundaries

**In scope:** presenting credentials on CLI→gateway calls, resolution
precedence, storage and permissions, actionable failures, unattended hook
operation, gateway-backed issuance.

**Out of scope:** the server-side registry (ADV-GATEWAY-012); an OAuth
device-code flow for humans — the bearer path stays supported precisely so
non-browser clients need no OAuth, and adding a device flow would be a second
credential model for one CLI (revisit only if humans start sharing one
bootstrap token, which is a smell in itself); console login (ADV-GATEWAY-010).

## Risk + Rollback

- Risk (`auth`): a CLI that stores tokens on disk is a credential at rest on
  every enrolled workstation. Least-privilege bindings limit the blast radius
  (a repo+scope+actor credential cannot read another actor's memories), and
  rotation is ADV-GATEWAY-012's runbook — but the advance must state where
  the file is and what protects it, not imply more safety than exists.
- Risk: a token leaking into the post-commit hook script, shell history, or
  CI logs. The hook must read stored configuration rather than embedding a
  value; tested.
- Risk: silent degradation — a CLI that quietly skips authentication when no
  credential is found would appear to work against a loopback gateway and
  fail confusingly against the real one. Missing credentials fail loudly.
- Rollback: revert branch; the CLI returns to unauthenticated calls, which
  work only against a trusted loopback gateway.

## Reviewability

`arrive score --base origin/advance/phase-011`: **49 [YELLOW]**.

## Evidence

- [x] tdd:red-green — `crates/totem-cli/tests/auth.rs` written first and
      observed failing on the missing module; green after. Six tests, of
      which two encode the failures this advance exists to prevent: a
      credential bound to *another* repo is never presented (it would earn a
      confusing 403 in place of an actionable local error), and a resolved
      credential never renders its token in `Debug` (which reaches logs and
      CI transcripts).
- [x] tests:unit — 68 workspace test blocks green; fmt and clippy clean.
- [x] enroll:executed — against the **deployed** gateway,
      `https://totem-dev.fly.dev`:
  - no credential → refused locally, naming `--token`, `TOTEM_TOKEN` and the
    store path, before any network call;
  - `--token` → `synced 1 system(s), 8 component(s), 44 advance(s)`;
  - `TOTEM_TOKEN` → same, reporting `authenticating as the credential from
    TOTEM_TOKEN`;
  - a forged token → **401** from the gateway.
- Not claimed: the hook running unattended against the deployed gateway.
      The hook is credential-free by design and resolves from the store, but
      no store credential exists on this machine yet (the bootstrap token
      lives in a file, not the store), so that path is untested. It becomes
      testable when ADV-GATEWAY-012's issuance endpoints exist.

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7;
  the deployed-enroll evidence is workstation evidence.

## Changes Made

### 2026-08-07 - test: [ADV-CLI-002] credential resolution and loud failure (red)
- crates/totem-cli/tests/auth.rs: six tests

### 2026-08-07 - feat: [ADV-CLI-002] authenticate, and reach HTTPS
- crates/totem-cli/src/auth.rs: new — precedence, `ResolvedCredential` with
  a hand-written `Debug` that redacts, an error naming the whole order
- crates/totem-cli/src/enroll.rs: bearer on the request; `repo_id_of` so the
  operator need not repeat the repo
- crates/totem-cli/src/main.rs: `--token`/`--repo`; resolve *before* work
- crates/totem-cli/src/hook.rs: documented credential-free by design
- crates/totem-cli/Cargo.toml: `rustls` — the CLI had no TLS at all

## Check for Understanding

1. The credential is resolved *before* the landscape is parsed. What would a
   user see if the order were reversed, and why does that matter more for a
   first-time user than for a returning one?
2. A stored credential bound to a different repo is refused locally rather
   than presented. What would the gateway have returned, and why is a local
   error more useful than that response?
3. The post-commit hook contains no token. Name two places a token written
   into `.git/hooks` would end up, and say what makes the hook's unattended
   execution the aggravating factor.
4. `ResolvedCredential` has a hand-written `Debug`. What does the derived one
   do, and which two destinations make that dangerous in a CLI specifically?
5. Six green tests existed while the CLI could not open an HTTPS connection
   at all. What do the enroll tests spawn, why does that shape hide the
   defect, and what kind of test would have caught it?
