---
advance:
  id: "ADV-GATEWAY-014"
  title: "Trim recall payloads: embeddings out of client responses"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-07T17:30:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 20
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["public_api"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Every `totem_recall` response carries each record's full embedding vector —
384 floats per record (EMB-004's pin). The caller is an LLM agent paying per
token, and the vector is useless to it: ranking already happened server-side,
and nothing a client does with a recalled memory needs its coordinates.

Found in the first real agent use of the deployed gateway (2026-08-07,
claude.ai connector): recall worked correctly and returned "full sparse
embeddings in the payload too — you may want to strip those from recall
responses to save tokens."

The cost is not incidental. At 384 floats rendered as JSON, a handful of
recalled records can outweigh the memories themselves, on the single most
frequent call in the dogfood trial. A memory system whose recall is
expensive to read gets used less, which is the failure mode the whole value
loop is meant to avoid.

## Behavioral Change

After this advance:

- `totem_recall` (MCP) and `POST /recall` (REST) return records **without**
  the `embedding` field. Everything else — content, scope, provenance,
  economics, governance — is unchanged.
- The omission is a deliberate response shape, not an accident of
  serialization: a test asserts no embedding appears in a recall response,
  so a future refactor that reinstates it fails rather than quietly
  restoring the cost.
- Measure and record the before/after payload size for a representative
  recall; a token-cost claim with no number attached is a guess.
- If any surface genuinely needs vectors (a curator job, an evaluation
  harness), it reads them through the store rather than the client API, and
  the advance records which callers were checked.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] tidy: preparatory refactoring (no behavior change)
- [ ] test: recall responses (MCP and REST) contain no embedding; the store
      still holds one and vector search still ranks
- [ ] feat: response-shape change on both surfaces

## Scope and Boundaries

**In scope:** the client-facing recall response shape on MCP and REST, and
the measurement.

**Out of scope:** what the store keeps (embeddings stay — vector search
needs them); the audit trail's own record view; any change to ranking.

## Risk + Rollback

- Risk (`public_api`): this removes a field from a published response. No
  known consumer reads it — the console renders content and scope, and the
  CLI does not recall — but the advance must confirm that by inspection
  rather than assumption, and say which consumers were checked.
- Risk: a client that *needs* vectors is silently broken. None is known to
  exist; if one appears it gets an explicit surface rather than a
  reinstated default.
- Rollback: revert branch; payloads return to their current size.

## Evidence

- [ ] tidy:preparatory
- [ ] tdd:red-green
- [ ] tests:unit
- [ ] payload-size: before/after bytes for a representative recall, measured

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7.

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation, grounded in the files actually
changed)
