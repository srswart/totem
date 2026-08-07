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
  risk_flags: ["migration", "deployment"]
  evidence: ["tdd:red-green", "tests:unit"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: in_progress
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

## What the work found: nothing recorded which model wrote a vector

The advance assumed re-embedding was a loop. It is, but it had no way to
know what to loop over.

Rows carried an embedding and no indication of its provenance. That makes a
**partially** re-embedded index indistinguishable from a uniform one — and
the distinction matters more than it first appears. Cosine distance between
vectors from two different models is not a degraded signal; it is not a
signal. An index holding both keeps returning results, in a confident order,
that means nothing. There is no error, no warning, and no symptom until
somebody acts on a ranking.

So schema **v11** adds `embedding_model`, and `embed()` stamps it — in the
one function that produces embeddings, rather than at each call site, so it
cannot be forgotten by a future caller.

It is `option<string>`, not `string`. Every row written before this migration
genuinely has an unknown space, and defaulting them to the current model
would assert something untrue about vectors a previous model wrote. Absent
means unknown, and the pass treats unknown as stale.

That label is what makes the pass **targeted** (only stale rows),
**idempotent** (a second run rewrites nothing), **resumable** (a failure
leaves the rows it already did), and **auditable** — `GET /admin/embedding`
can answer "is this index in one space?", which nothing could answer before.

## Two decisions worth disagreeing with

**A build that asks for the real model and cannot load it panics.** The
obvious alternative is to fall back to the stub and log loudly. Rejected: the
deployment would report healthy and keep serving recall, plausibly and
wrongly, and the log line would be read by nobody until a ranking was
trusted. A deployment whose entire purpose is semantic recall should not
start without it.

**Re-embedding is `POST /admin/reembed`, not a start-up migration.** DEP-001
makes the gateway the store's sole owner, so the pass must run inside that
process — a separate one-shot binary cannot open the database. But at boot it
would hold the health check open for the whole pass on a machine count of
one, and re-run on every restart with no operator deciding it should. As an
explicit call it can follow the backup this advance's risk section requires.

## Planned Implementation Tasks

- [x] branch / claim
- [x] test: embedder-identity surfaced; re-embed idempotence
- [x] feat: feature wiring in the image, re-embed pass, start-up banner
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

- [x] tdd:red-green — `crates/totem-store/tests/reembed.rs` was written and
      run red against methods that did not exist, then green. Same for
      `crates/totem-gateway/tests/embedding_admin.rs`.
- [x] tests:unit — 72 test blocks green across the workspace; clippy clean on
      both the host and wasm targets. **These prove the re-embed logic, not
      the model.** The default build does not link `fastembed` — that is what
      the feature is for — so no test in this suite has ever loaded
      BGE-small-en-v1.5.
- [ ] **golden-queries — pending.** EMB-004's paraphrase query, run against
      the deployed instance, is the only thing that shows the real model is
      in the path. Until it passes, this advance claims a re-embed mechanism
      and an image that is *intended* to carry the model, and nothing about
      recall quality.
- Not claimed: any measurement of load time or memory on the host. The
      advance asked for the numbers; they come from the deployment, not from
      here.

## A build failure the suite could not have caught

The first deployment attempt failed at link time. The prebuilt ONNX Runtime
that `fastembed` pulls in references libstdc++ 13+ symbols
(`_M_replace_cold`); Debian **bookworm** ships GCC 12, which does not define
them. The error is a `rust-lld` line naming a C++ mangled symbol inside
`ort-sys` and says nothing about Debian releases.

All three image stages moved to **trixie**, the runtime stage included — the
binary needs that libstdc++ at run time, not only at link time.

`cargo test --workspace` was green throughout, because nothing in the default
build links `fastembed`. This is the fifth defect in three days to live at a
boundary the suite does not cross (see
`docs/overnight-experiment/log.md`, 2026-08-08).

## Changes Made

### 2026-08-08 - the label, and the pass
- `crates/totem-core/src/record.rs`: `Content::embedding_model`.
- `crates/totem-store/src/schema.rs`, `migrate.rs`: migration v11.
- `crates/totem-store/src/embedding.rs`: `embed()` stamps the label.
- `crates/totem-store/src/memory.rs`: `embedding_models()` and
  `reembed_all()`, with `ReembedSummary`.
- `crates/totem-store/tests/reembed.rs`: the pass rewrites a foreign space,
  is a no-op on a second run, and **fails loudly** rather than reporting
  success over a still-mixed index when a row cannot be embedded.

### 2026-08-08 - the real model in the image
- `crates/totem-gateway/Cargo.toml`: a `fastembed` feature.
- `crates/totem-gateway/src/state.rs`: feature-selected embedder; a failure
  to load is a start-up failure, not a silent downgrade.
- `crates/totem-gateway/src/main.rs`: the start-up banner (the stub announces
  itself in as many words) and `--warm-embedder`, which exists so the image
  build pays the ~276s cold download instead of the first boot, which would
  fail its health check long before serving anything.
- `crates/totem-gateway/src/handlers.rs`, `lib.rs`: `GET /admin/embedding`
  and `POST /admin/reembed`, both authenticated — re-embedding rewrites every
  vector in the store.
- `Dockerfile`: trixie throughout, `--features rocksdb,fastembed`, weights
  baked at `/models`.

## Check for Understanding

1. Two models' vectors in one HNSW index. Recall still returns results, in a
   confident order. Why is that *worse* than an error, and what in this
   advance makes the condition detectable?
2. `embedding_model` is `option<string>` rather than a `string` defaulted to
   the current model. What would the defaulted version assert, and about
   which rows?
3. The label is stamped inside `embed()` rather than by each caller. What
   class of bug does that placement prevent, and what does it cost?
4. A build with `--features fastembed` that cannot load the model panics
   instead of falling back to the deterministic stub. Argue the other side,
   then say what the fallback would look like from outside the process.
5. Re-embedding is an endpoint, not a start-up migration. Which constraint
   forces it into the gateway process at all, and which two properties of the
   deployment rule out doing it at boot?
6. The whole suite was green while the image could not link. What does that
   say about what `cargo test --workspace` is evidence *of*?
