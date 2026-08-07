---
advance:
  id: "ADV-STORE-008"
  title: "Real embedder in the deployed build (BGE-small-en-v1.5)"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store", "gateway"]
  started_at: "2026-08-07T04:30:00Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["migration"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

Recall quality worth dogfooding: the deployed gateway still embeds with
`DeterministicEmbedder` (non-semantic). Enable the pinned model
(EMB-004: BGE-small-en-v1.5 via `fastembed`, 384 dims, cosine) in the
deployed build and re-embed existing rows so old and new memories rank in
the same space.

WORKSTATION advance: the model download is exactly what the cloud sandbox
egress blocks (EMB-002/EMB-003); the hosted build fetches it once.

## Behavioral Change

After this advance:

- The deployment image builds with the store's `fastembed` feature; the
  gateway logs which embedder it is running at start-up (the deterministic
  stub must announce itself, mirroring the EPHEMERAL banner's honesty).
- A re-embed pass exists (one-shot command or start-up migration — chosen
  and justified) that re-embeds every stored memory with the real model;
  mixed-space ranking is not allowed to persist silently.
- EMB-004's golden queries run against the deployed instance as a smoke
  check: the paraphrase query that defeats the lexical baseline ranks
  first, proving the real model is actually in the path.

## Planned Implementation Tasks

- [ ] branch / claim
- [ ] test: embedder-identity surfaced; re-embed idempotence
- [ ] feat: feature wiring in the image, re-embed pass, start-up banner
- [ ] golden-query smoke on the deployed instance, recorded verbatim

## Risk + Rollback

- Risk (`migration`): re-embedding rewrites every vector; back up first
  (the ADV-INFRA-002 runbook step precedes it).
- Risk: model load time and memory on a small host — record the numbers;
  if the host cannot hold the model, that is a finding for the hosting
  size, not a reason to ship the stub silently.
- Rollback: rebuild without the feature; vectors remain valid for the
  deterministic embedder only if re-embedded back — hence the backup.

## Evidence

- [ ] tdd:red-green
- [ ] tests:unit
- [ ] golden-queries: executed on the deployed instance

## Changes Made

- None yet

## Check for Understanding

(placeholder — written during implementation)
