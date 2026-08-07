---
advance:
  id: "ADV-GATEWAY-014"
  title: "Trim recall payloads: embeddings out of client responses"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway"]
  started_at: "2026-08-07T17:30:00Z"
  implementation_completed_at: "2026-08-07T18:30:00Z"
  review_time_estimate_minutes: 20
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 13
  risk_flags: ["public_api"]
  evidence: ["tests:unit", "tdd:red-green", "payload-size"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
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

- [x] branch / claim
- [x] tidy: none needed — `ops::recall` was already the single point both
      surfaces return through
- [x] test: recall carries no embedding (REST, and MCP through the same
      `ops` path); the store still holds vectors so search is unaffected
- [x] feat: strip in `ops::recall`; stop serializing an absent embedding

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

## Reviewability

`arrive score --base origin/advance/phase-011`: **13 [GREEN]**.

## Evidence

- [x] tdd:red-green — `crates/totem-gateway/tests/recall_payload.rs` written
      first and observed failing with the vector in the body; green after.
- [x] tests:unit — 67 workspace test blocks green; fmt and clippy clean.
- [x] payload-size — **measured, not estimated**: one recalled record went
      from **2,479 to 561 bytes (77% smaller)**. At ten records a call that
      is roughly 18 KB of vectors an agent no longer reads or pays for, on
      the most frequent call in the trial.
- [x] consumers checked, as the risk section required: `grep` across the
      console, CLI, and gateway found **no reader of `content.embedding`
      outside `ops`** — the console renders body and scope, the CLI does not
      recall. Nothing was broken by the removal; this was confirmed rather
      than assumed.

## Note on the second change

`Content::embedding` also gained `skip_serializing_if`, so an absent
embedding renders as nothing rather than `"embedding": null`. It is a
`totem-core` field, but the store persists it through `totem-store`'s own row
mapping rather than serde, so the change is wire-shape only — asserted by the
test that reads a vector back out of the store after a save.

## CI Evidence Notes

- Externally-run checks before merge per docs/cloud-agent-notes.md Step 7.

## Changes Made

### 2026-08-07 - test: [ADV-GATEWAY-014] recall carries no embedding (red)
- crates/totem-gateway/tests/recall_payload.rs: new — absence in the
  response, presence in the store

### 2026-08-07 - feat: [ADV-GATEWAY-014] strip embeddings from client responses
- crates/totem-gateway/src/ops.rs: `without_embeddings`, applied in
  `recall` so REST and MCP cannot drift on what a client receives
- crates/totem-core/src/record.rs: `skip_serializing_if` on
  `Content::embedding`

## Check for Understanding

1. The trim is applied in `ops::recall` rather than in the REST handler and
   the MCP tool separately. What failure does that placement prevent as new
   surfaces are added?
2. The store still holds every vector while responses carry none. Which
   store behaviour would break if the trim had been applied one layer
   deeper, and which test would have caught it?
3. `Content::embedding` gained `skip_serializing_if`, yet stored rows are
   unaffected. What does `totem-store` use to persist that field, and why
   does that make a serde attribute safe here but not in general?
4. The advance required the payload reduction to be measured rather than
   claimed. What were the two numbers, and why does an unmeasured
   token-cost claim deserve suspicion in an advance whose entire objective
   is cost?
5. Removing a field from a published response is a `public_api` risk. What
   did the advance require before accepting it, and what was actually
   found?
