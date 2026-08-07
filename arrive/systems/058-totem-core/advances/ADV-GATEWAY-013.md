---
advance:
  id: "ADV-GATEWAY-013"
  title: "OAuth 2.1 resource server: protected-resource metadata + third-party token validation"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-07T08:00:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 45
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth", "public_api"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: planned
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

- [ ] branch / claim
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: metadata document shape and reachability *without* a credential;
      401 carries `resource_metadata`; audience mismatch refused; expired
      refused; bad signature refused; unknown issuer refused; a valid token
      produces the expected grant; the static-bearer path still passes
- [ ] feat: metadata route outside the auth layer, `WWW-Authenticate`
      enrichment, JWKS-validating token verifier, claims→grant mapping

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

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] connector:executed — the ADV-GATEWAY-011 probe repeated end-to-end: a
      connector created in claude.ai against a deployed Totem, and a
      scheduled routine calling a Totem tool through it. This advance is not
      complete on unit tests alone; the thing it exists to enable must be
      demonstrated.

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7.
  The connector evidence is workstation/deployment evidence, like
  ADV-STORE-006's and ADV-INFRA-001's.

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation, grounded in the files actually
changed)
