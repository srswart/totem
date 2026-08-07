---
advance:
  id: "ADV-GATEWAY-011"
  title: "Investigation: cloud-routine connector reach to an external HTTPS MCP"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-07T04:30:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 15
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software]
  work_products: [test_automation]
  status: planned
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

## Outcome (to be filled by the run)

- [ ] A minimal MCP endpoint (can be the totem-gateway itself behind a
      temporary tunnel, or any trivial tool server) reachable over public
      HTTPS with a bearer credential.
- [ ] A test routine configured with the connector; one run demonstrating a
      successful tool call (and one demonstrating the 401 path with a bad
      credential).
- [ ] Findings recorded in docs/tech-direction/mcp.md §3 (per-harness matrix
      row moves from "unverified" to executed), including any constraints
      discovered (headers, auth mechanics, connector config shape).

## Risk Hypothesis

- H1: the connector mechanism passes a configured bearer header through to
  the MCP server. If it cannot, the auth model needs a rethink (query-param
  tokens are not acceptable; that failure would be a major finding).
- H2: the sandbox/harness imposes no additional egress restriction on
  connector-mediated calls (connectors run harness-side, not sandbox-side —
  expected true, worth executing).

## Scope and Boundaries

In scope: one round trip, both auth outcomes, the findings. Out of scope:
permanent hosting (ADV-INFRA-002), Totem tool semantics.

## Evidence

- [ ] investigation:findings

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
