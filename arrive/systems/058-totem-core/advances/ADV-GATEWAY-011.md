---
advance:
  id: "ADV-GATEWAY-011"
  title: "Investigation: cloud-routine connector reach to an external HTTPS MCP"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-07T04:30:00Z"
  implementation_completed_at: "2026-08-07T07:40:00Z"
  review_time_estimate_minutes: 15
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 21
  risk_flags: []
  evidence: ["investigation:findings", "tests:unit"]
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software]
  work_products: [test_automation]
  status: complete
---

## Objective

Prove (or refute) the load-bearing assumption of the dogfood plan
(docs/dogfood/plan.md §3.1): that a scheduled cloud routine can call a tool on
an external HTTPS MCP endpoint attached as a claude.ai connector.
docs/tech-direction/mcp.md verified Totem's streamable-HTTP transport
end-to-end but explicitly could not verify any harness's *outbound* connector
reach — this closes that gap before hosting money or effort is spent.

WORKSTATION advance: needs routine configuration (claude.ai side) and a
throwaway public HTTPS endpoint — both human-held resources.

## Outcome

**H1 refuted — and that is the finding.** claude.ai connectors cannot carry a
static bearer token, so ADV-GATEWAY-003's credentials alone cannot make Totem
reachable from a scheduled routine. Discovered in one workstation afternoon,
before any hosting was provisioned — which is exactly why this advance was
sequenced first.

- [x] The real gateway (durable store, ADV-GATEWAY-003 auth) exposed over
      public HTTPS via a throwaway `cloudflared` quick tunnel.
- [x] Round trip verified from the public side: MCP `initialize` succeeds
      with the bearer, anonymous is refused `401` — the transport and the
      auth layer both work over public TLS.
- [x] Connector attachment attempted through the claude.ai UI. The dialog
      offers **OAuth Client ID/Secret only**; Add triggered RFC 7591 Dynamic
      Client Registration, which failed against Totem verbatim: *"Couldn't
      register with Totem's sign-in service…"*
- [x] Discovery paths probed: `/.well-known/oauth-protected-resource` and
      `oauth-authorization-server` return 401 (auth sits in front of them),
      and our `WWW-Authenticate: Bearer` omits `resource_metadata`.
- [x] Findings recorded as **MCP-012, MCP-013, MCP-014** in
      docs/tech-direction/mcp.md §3, with a new executed matrix row for
      claude.ai scheduled routines and three options for the dogfood plan.
- [x] One fix shipped rather than merely noted: `TOTEM_MCP_ALLOWED_HOSTS`
      (MCP-012), since a loopback-only allowlist would block any deployment.

**A routine actually calling a Totem tool was NOT demonstrated** — the
connector could not be created, so there was nothing to attach. Recorded
plainly rather than counted as a partial success.

## Recommendation

Front the deployed gateway with an **OAuth proxy** (Cloudflare Access or
`oauth2-proxy`) rather than implementing OAuth 2.1 inside Totem: it satisfies
the connector, keeps a second auth implementation out of the security
surface, and fits the "secure without over-complication" constraint. Totem's
own bearer credentials stay exactly as built — they bind repo+scope+actor
behind whatever the proxy authenticates. ADV-INFRA-002 should terminate
OAuth; ADV-GATEWAY-012 should assume a proxy-supplied identity rather than
growing an OAuth server. Whatever is chosen, MCP-014 requires the discovery
documents to sit outside the auth layer.

## Risk Hypothesis (dispositions)

- **H1 — REFUTED.** The connector dialog has no header or bearer field at
  all; authentication is OAuth-only (Client ID/Secret, with DCR attempted
  first). The "major finding" branch this hypothesis anticipated is the one
  that occurred, and the auth model is being reshaped accordingly (see
  Recommendation) — before hosting spend, as intended.
- **H2 — not reached.** No connector could be created, so no
  connector-mediated call was made and no egress question arose. Left
  untested rather than assumed; it is only answerable once an OAuth front
  exists.

## Scope and Boundaries

In scope: one round trip, both auth outcomes, the findings. Out of scope:
permanent hosting (ADV-INFRA-002), Totem tool semantics.

## Reviewability

`arrive score --base origin/advance/phase-010`: **21 [GREEN]**. An
investigation plus one small production change (the allowlist env var,
shipped here because the probe proved the default blocks every deployment).

## Evidence

- [x] investigation:findings — MCP-012, MCP-013, MCP-014 in
      docs/tech-direction/mcp.md, each with verbatim error text and the
      executed path that produced it; per-harness matrix row added for
      claude.ai scheduled routines.
- [x] tests:unit — `cargo test -p totem-gateway` green (15 result blocks)
      with the allowlist change; fmt and clippy clean.
- Not claimed: `tdd:red-green` — investigation mode, and the allowlist fix
      was derived from an executed 403 rather than a written-first test.
- Not claimed: a routine calling a Totem tool. It did not happen.

## Changes Made

### 2026-08-07 - feat: [ADV-GATEWAY-011] TOTEM_MCP_ALLOWED_HOSTS
- crates/totem-gateway/src/mcp_http.rs: env-configured extension of rmcp's
  `allowed_hosts`; default stays loopback-only

### 2026-08-07 - docs: [ADV-GATEWAY-011] probe findings
- docs/tech-direction/mcp.md: MCP-012..MCP-014, executed matrix row for
  claude.ai scheduled routines, three options for the dogfood plan
- docs/dogfood/plan.md: §3.1 updated with the OAuth-front requirement

## Check for Understanding

1. The probe verified a successful MCP `initialize` over public HTTPS with a
   bearer token, and still concluded H1 was refuted. What is the difference
   between "the server accepts a bearer over TLS" and "the harness can send
   one", and which artifact in the connector dialog settles it?
2. `WWW-Authenticate: Bearer` is returned on 401, and the two
   `.well-known` OAuth documents return 401 as well. Why does that
   combination make it impossible for a spec-compliant client to
   authenticate even in principle, and what must change about
   ADV-GATEWAY-003's "everything behind auth" rule?
3. The recommendation is an OAuth proxy rather than OAuth inside the
   gateway. What does that choice preserve about ADV-GATEWAY-003's existing
   credential model, and what does it add to ADV-INFRA-002's scope?
4. `TOTEM_MCP_ALLOWED_HOSTS` extends rather than replaces rmcp's default
   list, and the default is loopback-only. What attack is that default
   defending against, and why is "extend, opt-in" the right shape for a
   deployment knob?
5. H2 is recorded as "not reached" rather than "passed". Why would recording
   it as passed have been a lie, and what would it take to answer it?
