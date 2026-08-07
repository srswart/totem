---
advance:
  id: "ADV-GATEWAY-013"
  title: "OAuth 2.1 resource server: protected-resource metadata + third-party token validation"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-07T08:00:00Z"
  implementation_completed_at: "2026-08-07T17:50:00Z"
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 82
  risk_flags: ["auth", "public_api"]
  evidence: ["tests:unit", "tdd:red-green", "connector:executed"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: complete
---

## Objective

**Spec reference:** MCP authorization spec, revision 2025-06-18 —
https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization
(fetched 2026-08-07; the requirements quoted below are from it).

Make Totem reachable from claude.ai scheduled routines — the reason the
dogfood trial exists — by implementing what the MCP authorization spec
actually requires of an MCP server. ADV-GATEWAY-011 established that
connectors are OAuth-only (MCP-013) and that our discovery documents are
hidden behind our own auth layer (MCP-014).

**The key scoping fact, from the spec:** an MCP server acts as an *OAuth 2.1
resource server*. The authorization server "may be hosted with the resource
server or a separate entity" and its implementation is explicitly out of
scope. **Totem does not implement an authorization server, does not issue
OAuth tokens, and does not run a login UI** — it points at a third-party
identity provider and validates the tokens that provider issues. That is a
far smaller and safer change than "implement OAuth 2.1", and it supersedes
ADV-GATEWAY-011's interim proxy recommendation: an in-gateway resource server
keeps authorization where the scope invariants already live, instead of
splitting it across a proxy.

## Behavioral Change

After this advance:

- **Protected Resource Metadata (RFC 9728), unauthenticated.**
  `GET /.well-known/oauth-protected-resource` returns the document naming
  Totem's canonical resource URI and its `authorization_servers`. This path —
  and only the discovery paths — sit *outside* the auth layer, a deliberate,
  tested exception to ADV-GATEWAY-003's "every route authenticated" (MCP-014).
- **A useful 401.** `WWW-Authenticate: Bearer resource_metadata="https://<host>/.well-known/oauth-protected-resource"`,
  per RFC 9728 §5.1, so a client can discover how to authenticate from the
  refusal itself.
- **Third-party access tokens validated as a resource server.** Signature
  verified against the provider's JWKS (cached, refreshed on unknown `kid`),
  `exp`/`nbf` enforced, issuer checked — and, non-negotiably, **audience
  validated**: a token whose `aud` is not Totem's canonical URI is refused,
  even if perfectly valid for another service. The spec calls unaudited
  audience the road to confused-deputy; the tests treat it as such.
- **Claims map to the existing grant model.** A validated token yields the
  same `TokenGrant` (repo + scope + actor) the bearer path produces, so
  every downstream authorization check, scope-chain resolution, and access-log
  entry is unchanged. Mapping rules (which claim carries the actor, how a
  repo/scope binding is derived or looked up) are recorded in the advance.
- **The existing static-bearer path stays.** `curl`, the CLI, Claude Code's
  own `--transport http` registration and the Claude API MCP connector all
  use it and all keep working; OAuth is an additional accepted credential
  form, not a replacement.

## Planned Implementation Tasks

- [x] branch / claim
- [x] tidy: none needed — `authenticate` already had a clean seam; OAuth
      slots in behind the registry lookup
- [x] test: metadata reachable without a credential, 401 carries
      `resource_metadata`, audience/issuer/expiry/signature refusals, valid
      token maps to a grant, static bearer still passes
- [x] feat: metadata route outside the auth layer, `WWW-Authenticate`
      enrichment, JWKS-validating verifier, claims→grant mapping

## Bug fix folded in: the first external client could not write

Deployed, connected, and immediately broken — `totem_save` failed for every
possible argument with:

    invalid type: string "{\"kind\": \"human\", \"actor\": \"...\"}",
    expected adjacently tagged enum Author

`SaveParams::author` was a `serde_json::Value`, which tells `schemars`
nothing, so the *published* schema declared no type. A client with no type to
work from serialized the argument as a string, and `Author` is adjacently
tagged with no string form — no caller could work around it.

**Every existing MCP test calls the tools in-process with typed Rust
arguments, so none of them ever exercised the schema an external client
reads.** Sixty-six green test blocks and an unusable write path. Fixed by
typing the parameters (`AuthorParam`, `SubjectParam`, `harness: String`) and
adding three schema tests — one of which fails on *any* untyped parameter,
catching the class rather than these two instances. A tolerant deserializer
also accepts a JSON-encoded string where an object is expected: the schema is
the fix, that is the hardening, since claude.ai will not be the last harness
to stringify an untyped field.

Folded into this advance rather than deferred because it was this advance's
own deployment that made an external client possible, and the advance's
completion bar requires a real tool call — which was impossible while the
write path was broken.

## Scope and Boundaries

**In scope:** resource-server behavior only — metadata, discovery-friendly
401, token validation, claims mapping.

**Out of scope:** any authorization-server function (no `/authorize`,
`/token`, `/register`, no login UI, no user database — the provider owns
all of it); provider selection and tenant configuration (an
ADV-INFRA-002 deployment concern); console human auth (reserved
ADV-GATEWAY-010).

## Provider: WorkOS AuthKit (decided 2026-08-07, Shawn)

Chosen over Auth0/Okta/self-hosted Keycloak: AuthKit documents the MCP
resource-server pattern directly, supports both Client ID Metadata Documents
(CIMD) and Dynamic Client Registration, and needs no service of our own to
run. Self-hosting an authorization server was rejected as more operations
than the entire rest of the deployment; building one inside Totem was
rejected as the security surface this advance exists to avoid.

The provider is named in exactly one place (the metadata document's
`authorization_servers`), so this choice stays swappable.

**Configuration the deployment supplies** (WorkOS dashboard →
*Connect* → *Configuration*, per workos.com/docs/authkit/mcp, fetched
2026-08-07):

- Enable **CIMD** (MCP client discovery) **and DCR** — the claude.ai
  connector was observed attempting DCR first (MCP-013), so both paths
  should exist.
- Register Totem's public MCP URL as a **valid Resource Indicator**, and set
  it as the default Resource Indicator so clients that omit `resource` still
  work.

**Values this advance consumes** (from the AuthKit domain, `$AUTHKIT`):

| Purpose | Value |
|---|---|
| Issuer (`iss` check) | `https://$AUTHKIT` |
| AS metadata (client-side discovery) | `https://$AUTHKIT/.well-known/oauth-authorization-server` |
| JWKS (signature verification) | `https://$AUTHKIT/oauth2/jwks` |
| Audience (`aud` check) | Totem's canonical resource URI |

**Protected Resource Metadata to serve** at
`/.well-known/oauth-protected-resource`:

```json
{
  "resource": "https://<totem-host>",
  "authorization_servers": ["https://<authkit-domain>"],
  "bearer_methods_supported": ["header"]
}
```

Configuration is environment-supplied, never compiled in: the issuer, JWKS
URL, and canonical resource URI are deployment values (workstation, host,
and test all differ), and the tests must be able to run against a fake
issuer without network access.

**Resolved 2026-08-07:** the hostname exists — Fly.io app `totem-dev`,
region `sin` (ADV-INFRA-002). Canonical resource URI
`https://totem-dev.fly.dev`, MCP endpoint `https://totem-dev.fly.dev/mcp`.
That is what registers as the WorkOS Resource Indicator and what the `aud`
claim is validated against, so this advance no longer waits on a hostname.

## Open decision: how many identities

If both routines authenticate as the same human, Totem sees one actor and the
per-routine identities (`cloud-opus`, `cloud-sonnet`) that ADV-INFRA-003
assumes collapse into one — the access log could no longer say which routine
recalled or saved what, weakening the dogfood measurement. Either provision
an AuthKit user per routine, or accept a single `cloud` actor and record the
loss plainly. Decide before the cutover, not during it.

## Risk + Rollback

- Risk (`auth`, `public_api`): this is the second credential path into the
  same store, and the first one a third party issues. Audience validation is
  the load-bearing check — a resource server that accepts any well-signed
  token becomes a confused deputy for every other service using that
  provider. It gets an explicit negative test, not an assertion of intent.
- Risk: opening the discovery paths is a deliberate hole in "everything
  behind auth". The metadata document is public by design and contains no
  secrets, but the exception must be *narrow* — exactly the `.well-known`
  paths, tested to prove nothing else slipped out with them.
- Risk: JWKS fetching adds an outbound dependency to the auth path; cache
  keys, and fail closed (refuse) rather than open when the provider is
  unreachable.
- Risk: token passthrough is forbidden by the spec — Totem must never
  forward a client's token to any upstream. Nothing does today; the advance
  should not introduce it.
- Rollback: revert branch. Static bearer credentials keep working, so the
  workstation, CLI and API-connector paths are unaffected; only claude.ai
  connector reach is lost.

## Reviewability

`arrive score --base origin/advance/phase-010`: **82 [RED]**, documented
rather than split. The OAuth module, its wiring, and the schema bug fix are
one deployment's worth of work on the same surface, and the bug was only
discoverable *after* the OAuth half shipped — splitting would have meant
merging a resource server whose own completion evidence could not be
obtained. The commit series is the review order: tests, feature, bug fix.

## Evidence

- [x] tdd:red-green — `crates/totem-gateway/tests/oauth.rs` written first and
      observed failing on the missing type and field; green after. Eight
      tests: metadata without a credential, `resource_metadata` in the 401,
      valid token → grant, and refusals for wrong audience, wrong issuer,
      expiry, and tampering — plus one asserting the static bearer path still
      works, so this advance cannot quietly break ADV-GATEWAY-003.
- [x] tests:unit — 66 workspace test blocks green; fmt and clippy clean.
      Unit tests use HS256 with a fixed key: what is under test is
      issuer/audience/expiry checking and claims mapping, none of which
      depends on the algorithm. **The RS256-over-JWKS path is not unit
      tested** — it is verified live, below.
- [x] connector:executed — the bar this advance set for itself, met on
      2026-08-07:
  - claude.ai custom connector created against
    `https://totem-dev.fly.dev/mcp` with **no client id supplied** —
    Dynamic Client Registration succeeded against WorkOS AuthKit, where the
    same step failed against Totem the day before (MCP-013).
  - Google sign-in through AuthKit; connector authorized.
  - `totem_recall` returned the project-scope estate, and the identity
    binding refused a request naming another actor verbatim: *"this
    credential is bound to actor user_01KZEG2Y2T1XN5M22DYKG65XCT, so it
    cannot act as claude"* — a WorkOS-issued token's claims enforcing
    ADV-GATEWAY-003's grant rules.
  - `totem_save` (after the schema fix) wrote a record whose provenance
    carries the **proven** AuthKit subject as author, not a caller-asserted
    string: `author: {kind: human, actor: user_01KZEG2Y2T1XN5M22DYKG65XCT}`,
    session `session_01UrhKGahJ1ipFJZSHFTDckQ`. Read back from the
    workstation with a *different* credential (the bootstrap bearer), which
    is the loop this advance exists to close.

## Residuals found and left open, deliberately

- **A save that succeeds without acknowledging.** The connector's
  `totem_save` completed server-side while claude.ai waited indefinitely for
  a response. Not investigated here — but it matters more than it looks: an
  agent that never sees an acknowledgement retries, and duplicate memories
  are exactly what a curator later has to clean up. Recorded for a follow-up
  rather than hand-waved as a UI quirk.
- **Recall payload size** — full 384-float embeddings returned to a caller
  that pays per token. Authored as ADV-GATEWAY-014 rather than folded in;
  this advance had already absorbed one unplanned fix.
- **Per-routine identity is still unproven.** Both cloud routines would
  authenticate as whichever AuthKit user connects them, so `cloud-opus` and
  `cloud-sonnet` may collapse into one actor. The claims mapping makes the
  *token's* subject the actor, so two AuthKit users would separate them —
  untested, and ADV-INFRA-003's measurement depends on the answer.

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7.
  The connector evidence is workstation/deployment evidence, like
  ADV-STORE-006's and ADV-INFRA-001's.

## Changes Made

### 2026-08-07 - test: [ADV-GATEWAY-013] resource-server metadata, 401 discovery, token validation (red)
- crates/totem-gateway/tests/oauth.rs: eight tests, including the
  static-bearer regression guard

### 2026-08-07 - feat: [ADV-GATEWAY-013] OAuth 2.1 resource server
- crates/totem-gateway/src/oauth.rs: new — RFC 9728 metadata, JWKS cache
  refreshed on unknown `kid` (how key rotation survives without a restart),
  audience/issuer/expiry validation, claims→grant mapping
- crates/totem-gateway/src/lib.rs: metadata route in
  `unauthenticated_routes()` beside `/health`
- crates/totem-gateway/src/auth.rs: OAuth fallback behind the registry
  lookup; `InvalidToken` variant (401, not the 403 `InvalidBinding` gives —
  a client told 403 believes its permissions are wrong and never refreshes);
  `with_discovery` attaches `resource_metadata` on the way out, so refusals
  from deeper handlers carry it too
- crates/totem-gateway/src/main.rs: `oauth_from_env`, startup mode line
- fly.toml: AuthKit issuer, resource, repo and scope — none secret

### 2026-08-07 - fix: [ADV-GATEWAY-013] typed MCP save parameters
- crates/totem-gateway/src/mcp.rs: `AuthorParam`, `SubjectParam`,
  `harness: String`, tolerant `json_or_stringified` deserializer
- crates/totem-gateway/tests/mcp_recall_and_save.rs: published-schema tests,
  including one that fails on any untyped parameter

## Check for Understanding

1. Totem publishes protected-resource metadata and validates tokens, but
   issues none and runs no login UI. Which sentence in the MCP authorization
   spec makes that split legitimate, and what would Totem have had to build
   if the spec required the two roles together?
2. Audience validation is called the load-bearing check. Construct the
   attack that succeeds if a resource server verifies signature, issuer and
   expiry but not audience — and name the WorkOS-shaped precondition it
   needs.
3. Validation failures return 401 via a new `InvalidToken` variant rather
   than reusing `InvalidBinding`. What does the existing variant's own doc
   comment say about when it applies, and what does a client do wrong if it
   receives 403 for an expired token?
4. The metadata document and `/health` are the only unauthenticated routes.
   Why can neither of them be moved behind the auth layer, and what does
   `unauthenticated_routes()` buy that per-route exceptions would not?
5. Sixty-six test blocks were green while the write path was unusable by any
   external client. What did every one of those MCP tests do that made the
   defect invisible, and which new test would fail if someone reintroduced
   the same shape on a different parameter tomorrow?
