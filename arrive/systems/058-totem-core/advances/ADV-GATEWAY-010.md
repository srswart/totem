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
  reviewability_score: 85
  risk_flags: ["auth", "public_api", "large_diff"]
  evidence: ["tests:unit", "tdd:red-green"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software, security]
  work_products: [production_code]
  status: in_progress
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

## What the browser forced: a same-origin token relay

The recommended approach was taken — the console is a public client doing
PKCE, and the gateway issues no session cookie and holds no client secret.
One part of it did not survive contact with a real browser.

**AuthKit's token endpoint cannot be called from a browser.** Its CORS
preflight answers `Access-Control-Allow-Origin: *`, so the browser sends the
POST; the *actual* response then omits the header, so the browser completes
the exchange and refuses to let the page read the result. The token is issued
and thrown away. Nothing on the client can repair a missing response header —
not a different fetch mode, not a header on the request.

So the exchange goes through `POST /console/token` on the gateway's own
origin, which forwards it to AuthKit and returns the response verbatim.

This is a deviation from "the gateway is a pure resource server" and is
recorded as one rather than described as unchanged. What it is *not* is the
rejected alternative: no client secret exists, no cookie or session is
issued, no token is minted here, and the gateway learns nothing it could
replay — PKCE still proves the exchange, because the verifier is generated
in the tab and only that tab can complete the code it started. The route is a
relay, and the destination comes from server configuration rather than from
the request body, so it cannot be pointed at an arbitrary host.

The narrower reading: the gateway is a pure resource server *for the tokens
it validates*. This route neither issues nor validates one.

## Planned Implementation Tasks

- [x] branch / claim
- [ ] tidy: preparatory refactoring — **none was needed**; the code this
      advance touched was either new or extended at its edges
- [x] test: static serving does not shadow API routes; unauthenticated
      console state renders a sign-in prompt; the API client attaches the
      bearer when a token is held and omits it when not
- [x] feat: wasm build stage in the Dockerfile, static serving with SPA
      fallback, PKCE login flow, token handling, signed-out UI
- [x] fix: the same-origin token relay and the dashboard flash, both found by
      an actual sign-in

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

## Reviewability: Red at 85, and why it was not split

`arrive score --base a9ce434` reports **85 [RED]** (size 56, risk 15,
novelty 14) across 15 files and ~800 added lines. The budget says split
unless documented, so: **it should have been split and was not.** "Serve the
console from the gateway" and "sign a human in" are separable — the first is
a Dockerfile stage, a static mount and a shadowing test, and it is
independently deployable and independently reviewable.

The honest reason is that the split was not considered until the diff was
already large. The retroactive argument for one advance — that the evidence
is a human signing in, which needs both halves deployed together — is true
but would have been satisfied by merging the serving advance first and taking
the login evidence on the second.

A reviewer should read it in that order: `Dockerfile` + `lib.rs` +
`tests/console_serving.rs` first (serving, and the shadowing guarantee), then
`auth.rs` + `handlers.rs` (the flow).

## Evidence

- [ ] **tidy:preparatory — not claimed.** No preparatory refactoring was
      done. The advance's own task list asked for it; there was nothing to
      tidy, and an empty tidy commit would be evidence of nothing.
- [x] tdd:red-green — `tests/console_serving.rs` was written and run red
      (commit 8db0105, before any serving existed) and went green with
      55f59c2. The PKCE helpers in `auth.rs` were tested alongside, not
      before; that half is green-after, and only the serving half earns the
      red-first claim.
- [x] tests:unit — 70 test blocks green across the workspace, including four
      host tests for the PKCE helpers (one checks RFC 7636's own worked
      example, so the challenge derivation is verified against the spec
      rather than against itself) and four HTTP tests asserting the console
      is served *and* that API paths are never shadowed by the SPA fallback.
- [ ] login:executed — **pending.** An earlier attempt reached AuthKit,
      authenticated through Google, and came back — which is what found both
      defects in the final commit; nothing in the suite could have. It then
      failed at the token exchange, so no sign-in has yet completed. This
      stays unchecked, and the advance stays `in_progress`, until one does.
- Not claimed: that the token store is safe against a compromised page. A
      browser-held token is reachable by any script on the page;
      `sessionStorage` narrows the window to the tab's lifetime and that is
      the whole of the mitigation.

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7;
  the login evidence is deployment evidence and cannot come from CI.

## Changes Made

### 2026-08-07 - the console is served from the gateway
- `Dockerfile`: a `console` build stage — `wasm32-unknown-unknown`,
  `dioxus-cli 0.7.3`, `dx build --platform web --release`. It needs
  `pkg-config`, `libssl-dev` and `build-essential`; without them the
  `dioxus-cli` build fails on OpenSSL, which reads as an unrelated error.
- `crates/totem-gateway/src/lib.rs`: `console_service()` mounted with
  `fallback_service`, so it is reached only when no route matched. The
  shadowing hazard is therefore structural, not a matter of ordering — and
  `tests/console_serving.rs` asserts it anyway, because that failure would
  look like a broken client rather than a routing bug.
- `crates/totem-gateway/src/state.rs`, `main.rs`, `fly.toml`: the console
  directory and OAuth client configuration, supplied by environment rather
  than compiled in, so one wasm bundle runs on a workstation and on the
  deployment.

### 2026-08-07 - PKCE sign-in
- `crates/totem-console/src/auth.rs`: new — verifier and challenge
  derivation, the authorization URL (carrying RFC 8707 `resource=`, without
  which AuthKit may issue a token the gateway correctly refuses and the
  failure looks like a login bug), state validation across the redirect, and
  `sessionStorage` handling.
- `crates/totem-gateway/src/handlers.rs`: `/console/config` publishes the
  issuer, client id, redirect URI and resource. Unauthenticated by
  necessity — a page that cannot authenticate yet must be able to read it —
  and added to `unauthenticated_routes()`, the single reviewable list where
  that decision stays visible. It contains nothing secret; every value in it
  appears in the authorization URL in the user's address bar.
- `crates/totem-console/src/api.rs`: an `authorized()` wrapper attaches the
  bearer to every call, and the free-text actor field is gone — the
  identity now comes from the token, retiring the risk ADV-CONSOLE-001
  recorded.

### 2026-08-08 - what an actual sign-in found
- `crates/totem-gateway/src/handlers.rs`, `dto.rs`, `lib.rs`:
  `POST /console/token` relays the code exchange from our own origin. See
  the section above for why the browser leaves no alternative and why this
  is not the rejected gateway-as-OAuth-client design.
- `crates/totem-console/src/auth.rs`: exchange through the relay.
- `crates/totem-console/src/api.rs`: while deciding whether the tab held a
  session, `RootApp` rendered the **dashboard** — so every load flashed a
  dashboard before the sign-in card. The comment beside it claimed the
  three-state split existed to prevent exactly that. It now renders nothing
  until the answer is known.

## Three identifiers, three mix-ups

Worth recording because each cost a deploy cycle and each failed in a way
that pointed elsewhere:

1. **The WorkOS *Application* id is not an OAuth client id.** An Application
   in the WorkOS dashboard is not a registered client of AuthKit's
   authorization server; using its id returns `application_not_found`, which
   reads as "your app is missing" rather than "wrong kind of identifier". The
   working client id came from Dynamic Client Registration against
   AuthKit's `/oauth2/register`; `fly.toml` records the exact call.
2. **The redirect URI must match exactly**, including scheme and trailing
   path, and it is registered with the *client*, not the Application.
3. **The RFC 8707 `resource` must match an audience the gateway accepts.**
   Here it is `https://totem-dev.fly.dev/mcp`; `oauth.rs::from_env` derives
   the bare origin as well and accepts **both**, precisely because the MCP
   spec lets a client send either and the mismatch is invisible from
   outside. What it is never is the *issuer* — sending that yields a token
   the gateway correctly refuses, and the failure presents as a login bug.

## Check for Understanding

1. AuthKit's token endpoint answers the CORS *preflight* permissively and
   then omits the header from the real response. Why does that mean the
   exchange succeeds at AuthKit and still fails in the console, and why can
   no client-side change fix it?
2. `POST /console/token` forwards a code exchange to the authorization
   server. Name the three properties that keep this from being the
   "gateway-mediated login" alternative this advance rejected — and the one
   sentence in which the gateway is honestly no longer a *pure* resource
   server.
3. The relay takes the destination from server configuration and only the
   code and verifier from the request. What attack does that ordering
   prevent, and what would go wrong if the issuer came from the body?
4. `/console/config` is unauthenticated and publishes the client id. Why is
   that not a leak, and what property of a *public* OAuth client makes the
   question well-posed rather than a matter of trust?
5. The console renders nothing while it decides whether the tab has a
   session. What did it render before, and why did a code comment asserting
   the opposite survive review?
6. This advance scored 85 (Red) and was not split. Where was the natural
   seam, and what would taking it have cost in evidence?
