---
advance:
  id: "ADV-GATEWAY-010"
  title: "Console on the gateway: static hosting + AuthKit browser login"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "console"]
  started_at: "2026-08-07T15:00:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 40
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

Make the console reachable by a human at `https://totem-dev.fly.dev` and able
to *authenticate*. Two gaps found while deploying ADV-INFRA-002:

1. **The console is not deployed at all.** The Fly image builds only
   `-p totem-gateway`; the Dioxus wasm bundle exists solely under `dx serve`
   on a workstation.
2. **The console cannot authenticate.** It sends no credential
   (`src/api.rs` issues bare `Request::get`/`post`), so against the deployed,
   authenticated gateway every call returns 401. Today's demos only work
   because a scratch binary serves the *trusted* loopback router — a
   composition with no deployment behind it.

This advance depends on ADV-GATEWAY-013: once the gateway validates AuthKit
tokens as an OAuth resource server, the console can use the *same* identity
provider and the *same* validation path, rather than inventing a second
credential model for humans.

## Behavioral Change

After this advance:

- **The console ships in the gateway image.** A wasm build stage produces the
  bundle; the gateway serves it as static files with an SPA fallback, mounted
  so it cannot shadow an API route (a request for a real API path must never
  return `index.html` — tested explicitly, because that failure looks like a
  broken client rather than a routing bug).
- **Same origin, therefore no proxy.** The console's relative `/recall`,
  `/landscape`, `/promotions` calls hit the same host, which retires the
  hand-maintained `[[web.proxy]]` entries in `Dioxus.toml` (dev-only) and
  avoids CORS entirely.
- **Human login through AuthKit.** Visiting the console unauthenticated
  starts the OAuth 2.1 authorization-code flow with PKCE against the
  AuthKit domain; the user signs in at WorkOS and returns to the console,
  which holds the access token and sends it as a bearer on every API call.
  The gateway validates it exactly as it validates an agent's token — one
  code path, one set of scope rules.
- **The scope chain still governs.** Authentication says *who*; the actor and
  project the token resolves to still determine what is visible. A logged-in
  human sees precisely what that identity's scope chain allows — the free-text
  actor field that let anyone read anyone's memories (ADV-CONSOLE-001's
  recorded risk) is replaced by the authenticated identity and that risk is
  retired in this advance's record.
- **Signed-out state is honest**: the console shows a sign-in prompt rather
  than empty views or a wall of 401 errors.

## Approach: public client with PKCE (recommended), and the alternative

**Recommended — the console is a public OAuth client.** The browser performs
the authorization-code + PKCE flow directly; no client secret exists to leak
into a wasm bundle, and the **gateway stays a pure resource server**, which is
the whole thesis of ADV-GATEWAY-013. Requires registering the console's
redirect URI (`https://totem-dev.fly.dev/callback` or similar) in WorkOS, and
implementing the flow in Rust/wasm.

**Alternative — gateway-mediated login.** The gateway acts as a confidential
OAuth client, runs the flow server-side, and issues a session cookie. Less
code in wasm, but it makes the gateway an OAuth client *and* a resource
server, adds a third credential path (bearer JWT, agent bearer, cookie) to a
security surface we have deliberately kept narrow, and puts a client secret
into the deployment. Choose this only if the wasm PKCE implementation proves
disproportionate, and record why.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: static serving does not shadow API routes; unauthenticated
      console state renders a sign-in prompt; the API client attaches the
      bearer when a token is held and omits it when not
- [ ] feat: wasm build stage in the Dockerfile, static serving with SPA
      fallback, PKCE login flow, token handling, signed-out UI

## Scope and Boundaries

**In scope:** shipping the console in the image, static serving, the browser
login flow, attaching the token to API calls, signed-out UI, retiring the
free-text actor field.

**Out of scope:** token *validation* (ADV-GATEWAY-013 owns it); refresh-token
rotation beyond what the flow needs for a trial; multi-user administration or
roles inside the console; the visual design system (ADV-CONSOLE-004, done).

## Risk + Rollback

- Risk (`auth`): a token in a browser is exposed to anything that can run
  script on the page. Keep it out of `localStorage` if a safer store is
  available in this stack, keep the console free of third-party script, and
  record what was actually done rather than what was intended.
- Risk (`public_api`): static-file serving with an SPA fallback is a
  classic route-shadowing hazard — a greedy fallback can swallow API paths or
  expose files that were never meant to ship. Both directions get tests.
- Risk: `/health` and the `.well-known` documents must stay unauthenticated
  while the console's own routes do not silently join them. The
  `unauthenticated_routes()` list added by ADV-INFRA-002 is where that stays
  reviewable; adding to it is a security decision.
- Rollback: revert branch; the gateway serves the API only and the console
  returns to `dx serve` against a loopback trusted gateway, which is the
  current state.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] login:executed — a human signing in at AuthKit and reaching the
      deployed console with real data, screenshotted. Unit tests cannot
      demonstrate the thing this advance exists for.

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7;
  the login evidence is workstation/deployment evidence.

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation, grounded in the files actually
changed)
