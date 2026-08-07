---
advance:
  id: "ADV-CLI-002"
  title: "CLI authenticates to the gateway"
  system: "058-totem-core"
  primary_component: "cli"
  components: ["cli", "gateway"]
  started_at: "2026-08-07T15:30:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: planned
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

- [ ] branch / claim
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: precedence order honored; 401 produces the actionable message;
      token never appears in output or in a world-readable file; hook-driven
      sync authenticates unattended
- [ ] feat: credential resolution, header on every gateway call, gateway-backed
      issuance, persisted gateway configuration for the hook

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

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] enroll:executed — `totem enroll` against the deployed gateway with a
      real credential, and the landscape read back from it.

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7;
  the deployed-enroll evidence is workstation evidence.

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation, grounded in the files actually
changed)
