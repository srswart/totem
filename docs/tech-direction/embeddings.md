# Tech Direction: Embedding provider + placement

**Status:** Findings accepted, closed (EMB-004, ADV-STORE-007) · **Date:** 2026-08-05 ·
**Advances:** ADV-STORE-003, ADV-STORE-007 · **Companion:**
[solution-intent.md](../solution-intent.md) §2.1, §9 · to be implemented by
`ADV-STORE-002` (currently `status: planned`)

Solution Intent §9 leaves two open questions for every Totem memory record's
`embedding` field: which **provider** produces it (local model vs. API), and
where **placement** happens (`totem-gateway` on write vs. `totem-curator` in a
batch job). This document records what a spike could and could not measure in
this environment, and the recommendation ADV-STORE-002 should implement.

**Verdict: a local model — `fastembed` with BGE-small-en-v1.5, 384
dimensions — embedded synchronously at gateway-write time; API-based
embedding stays an explicit non-default opt-in.** ADV-STORE-003 recommended
this shape on published specs alone (the sandbox egress policy blocks the
model download, EMB-002/EMB-003); **ADV-STORE-007 has since executed the
recommended model on a workstation and measured it (EMB-004)**: it ranks all
five labeled queries first — including the paraphrase the lexical baseline
misses — at ~5 ms per embedding. The decision ADV-STORE-002 implements is
now measured, not argued. The API candidates remain unmeasured, accepted:
they were already disfavored on the privacy ground alone (§4), so no
API-side measurement is planned.

**Pinned for downstream advances:** model `BGE-small-en-v1.5` via
`fastembed` (`=5.17.4` in the spike), **384 dimensions, cosine distance** —
ADV-STORE-001's HNSW index derives `DIMENSION 384` from this pin; a model
change re-opens both the pin and every stored vector (curator re-embeds).

## 1. What was run

| | |
|---|---|
| Code | `crates/totem-embedding-spike` — comparison harness, one real candidate, one reachability probe |
| Local candidate | `HashingEmbedder`: character-trigram hashing trick, 256 dims, L2-normalized, cosine similarity |
| Corpus | 10 memory-sized texts across Knowledge/Instructions/Context categories (`src/lib.rs::corpus`) |
| Quality run | `cargo test -p totem-embedding-spike` (3 tests, offline, deterministic) |
| Latency/readout | `cargo run -p totem-embedding-spike --example embed_corpus` |
| Reachability probe | `cargo run -p totem-embedding-spike --example probe_api_reachability` (manual, not part of `cargo test --workspace`) |

Only one candidate — the local hashing trick — actually ran an embedding and
got measured. The other two candidates in scope (API-based, local pretrained
model) could not be executed here; §2 and §3 say exactly what was verified
instead.

## 2. The load-bearing findings

### EMB-001 — A purely lexical local embedder retrieves correctly only when the query shares vocabulary with its target. *(confirmed, by execution)*

`HashingEmbedder` hashes character trigrams into a fixed-size bag and
L2-normalizes, so cosine similarity is a plain dot product over shared
trigrams — no semantic model, just surface overlap.

- On four queries built to share words with their expected match (e.g. "Why
  shouldn't I push directly to master?" against the record containing "push
  directly to master"), it ranks the expected record **first every time**
  (`hashing_embedder_wins_on_lexical_overlap_queries`).
- On one query paraphrased away from its target's vocabulary — "How do we
  keep private data out of shared scopes?" against a record that says "Scope
  isolation must be enforced at the store layer, not filtered in the
  gateway" — it ranks the expected record **second**, not first
  (`hashing_embedder_loses_on_paraphrased_query_with_no_shared_vocabulary`;
  readout in `embed_corpus`'s output, `rank_of_expected=Some(2)`).

One paraphrase case is not a benchmark, and this doesn't prove a semantic
model would rank it first either — no semantic model was measured (§3). What
it does prove, by execution rather than by argument, is that a purely lexical
local baseline degrades exactly where it's expected to: real agent recall
queries are closer to the paraphrase than to the keyword-overlap cases, since
`totem_recall(query, ...)` takes natural-language questions, not keyword
search terms (Solution Intent §3.1).

### EMB-002 — Every embedding API host tested is blocked by this sandbox's egress policy; a same-region control host is not. *(confirmed, by execution)*

`probe_api_reachability` sent a real HTTPS GET, through `HTTPS_PROXY` if set,
to four candidate API hosts and one control host known to be allowed (see
`/root/.ccr/README.md`'s `noProxy` list). Captured output:

```
== embedding API host reachability (HTTPS_PROXY honored if set) ==
  control      https://index.crates.io/config.json           -> HTTP 200 in 752.363069ms
  openai       https://api.openai.com/v1/embeddings          -> transport error in 336.415936ms: https://api.openai.com/v1/embeddings: Proxy failed to connect
  cohere       https://api.cohere.com/v1/embed               -> transport error in 581.702967ms: https://api.cohere.com/v1/embed: Proxy failed to connect
  voyageai     https://api.voyageai.com/v1/embeddings        -> transport error in 327.362651ms: https://api.voyageai.com/v1/embeddings: Proxy failed to connect
  huggingface  https://huggingface.co/                       -> transport error in 339.512289ms: https://huggingface.co/: Proxy failed to connect
```

The control host succeeds, so the agent and network path work; every
embedding-API host fails identically (`Proxy failed to connect`), matching a
plain `curl` CONNECT to the same hosts returning `403` — the same class of
block ADV-STORE-004 found for `download.surrealdb.com`. This means:

- No API-based candidate's retrieval quality, latency, or actual per-call
  cost could be measured here. §4's API cost figures are unverified secondary
  sources, not measurements.
- A gateway or curator running in *this* sandbox literally cannot call an
  embedding API — which is itself evidence for the "offline behavior"
  comparison criterion, though it's a property of this investigation
  environment, not necessarily of Totem's production deployment (unknown,
  and out of scope for this advance).
- Local, pretrained-model candidates that fetch weights from a hub at first
  use (§3) are equally blocked here, for the same reason.

### EMB-003 — A local pretrained-model candidate exists but could not be spiked in this environment. *(not executed — environment-blocked)*

`fastembed` (crates.io) is a pure-Rust, synchronous, no-Tokio library for
local ONNX embedding inference; by design it downloads its model (default
BGE-small-en-v1.5) to a cache directory on first use, then runs fully
offline. That first-use download is exactly the kind of request EMB-002
found blocked — so this candidate could not be added as a workspace member
and exercised the way `HashingEmbedder` was. This claim is sourced from a web
search of the crate's own documentation and README content (not
independently verified by running it), listed under Evidence below.

Closing this gap needs either an environment whose egress policy allows a
one-time model download, or a container image that bakes the model weights
in ahead of time — the same shape of fix ADV-STORE-006 identified for
SurrealDB server-mode parity.

### EMB-004 — The recommended model, executed: BGE-small-en-v1.5 ranks every labeled query first, including the paraphrase, at ~5 ms per call. *(confirmed, by execution — ADV-STORE-007)*

Run on a developer workstation (Docker-capable host with hub access — the
same environment split ADV-STORE-006 used), behind the spike's opt-in
`local-model` feature, against the identical corpus, labeled queries, and
harness as EMB-001:

- **Retrieval:** 5/5 queries rank their expected record first — including
  `"How do we keep private data out of shared scopes?"`, the paraphrased
  query the hashing baseline ranks second (EMB-001). Sensitivity control in
  the same test run: the hashing baseline still fails that query under the
  identical harness, so the comparison retains its discriminating case.
- **Dimensionality:** asserted 384 at run time; ADV-STORE-001's index pin is
  derived from this assertion, not from documentation.
- **Latency:** mean ~4.8–5.0 ms per embedding across the corpus
  (single-threaded CPU ONNX, Apple-silicon workstation, debug build) — well
  inside a synchronous gateway-write budget. Model construction: ~276 s on a
  cold cache (one-time weight download) vs ~124 ms warm; services must load
  once at startup, never per request, and a cold cache needs hub access
  (in a sandboxed deployment: bake the weights into the image).
- Two consecutive runs reproduce the ranking and latency.

One caveat carried honestly: the corpus is 10 records and 5 queries — a
calibration fixture, not a benchmark. Corpus-scale retrieval quality remains
ADV-STORE-005's question.

## 3. Comparing the three candidates against the stated criteria

| Criterion | Local hashing trick (measured) | Local pretrained model (not executed, EMB-003) | API-based (not executed, EMB-002) |
|---|---|---|---|
| Quality | Lexical-only; wins on vocabulary overlap, loses on paraphrase (EMB-001) | Expected semantic, unverified here | Expected semantic, unverified here |
| Latency | ~12 µs/call, single-threaded, no I/O (`embed_corpus` readout) | Unmeasured; ONNX CPU inference, expected single-digit ms per call, unverified | Unmeasured; adds network round trip on top of provider inference, unverified |
| Cost | $0 | $0 marginal (compute only) | Non-zero per token; e.g. one aggregator search (not independently verified, see below) quoted OpenAI `text-embedding-3-small` at $0.02/1M tokens standard, $0.01/1M batched, as of this document's date |
| Offline behavior | Fully offline by construction | Offline after first-use model download; that download is blocked here (EMB-002/EMB-003) | Requires network per call; blocked entirely in this environment (EMB-002) |
| Data leaves the deployment perimeter | No | No | Yes — every embedded memory body, including private `actor`-scope content, would be sent to a third party |

The cost figure is quoted from a live web search of embedding-pricing
aggregator sites, not from OpenAI's own pricing page (not independently
fetched — see EMB-002) and not from a measured API call. Treat it as a
directional signal, not a verified number.

## 4. Recommendation

**Provider: a local pretrained model (candidate EMB-003's shape — e.g.
`fastembed` with a small sentence-embedding model), not the hashing trick and
not an API.** The hashing trick is real and cheap but EMB-001 shows it misses
paraphrased queries, which is the normal shape of agent recall traffic. An
API adds a real privacy concern the hashing trick and local models don't:
Totem's own project brief names cross-scope leakage as the highest-severity
failure, and while sending private memory content to a third-party API isn't
literally a scope leak, it's the same shape of concern — private content
leaving the deployment boundary — and every actor-scope Knowledge/Instruction
record would be exposed to it on every write. A local model keeps content
inside the deployment and, per EMB-002, is the only path this environment
could plausibly close first (a one-time model bake, not a per-call network
dependency).

**Placement: gateway, on write, not curator batch.** The common record shape
(Solution Intent §2.1) stores `embedding` alongside `body` — a record without
one can't be recalled by vector search, so leaving it to a batch job means a
window where freshly-written memories are invisible to `totem_recall`.
Curator's stated role is maintenance of *existing* memory (dedupe,
consolidation, decay — Solution Intent §5), not the first-write path;
folding embedding generation into curator would make every new memory's
recall latency depend on the curator's batch cadence. `totem-curator` should
instead own **re-embedding**: a batch job that runs when the model version
changes, since every stored vector must stay comparable to the model that
produced it.

**Residual risk: resolved for the recommended candidate.** ADV-STORE-007 ran
this spike's harness against `fastembed`/BGE-small-en-v1.5 (EMB-004): quality
and latency for the recommended candidate are now measured. The API
candidates' columns remain unverified — accepted, since the privacy ground
(§4) disfavors them regardless of their quality.

## 5. What this spike deliberately did not answer

- Real embedding quality from any actual model (local pretrained or API) —
  blocked by EMB-002/EMB-003, not attempted.
- Re-embedding strategy mechanics (how a curator batch job detects a model
  version change and walks existing records) — left to ADV-STORE-002.
- Retrieval quality at corpus scale — this spike's corpus has 10 records;
  ADV-STORE-005's synthetic corpus is the intended scale test.
- Actual API-provider cost or latency — no call reached a provider (EMB-002).

## Evidence

- `cargo test -p totem-embedding-spike` — 3/3 pass, offline, no network
  (EMB-001).
- `cargo run -p totem-embedding-spike --example probe_api_reachability` —
  captured output above (EMB-002).
- `cargo run -p totem-embedding-spike --example embed_corpus` — retrieval and
  latency readout behind EMB-001's numbers.
- Web search, 2026-08-05, for `fastembed` crate documentation (EMB-003) and
  OpenAI embedding pricing (§3's cost row) — secondary sources, not
  independently verified against vendor primary sources or a real API call.
