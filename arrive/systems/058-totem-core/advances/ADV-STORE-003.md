---
advance:
  id: "ADV-STORE-003"
  title: "Investigation: embedding provider + placement spike"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-05T10:09:37Z"
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 68
  risk_flags: []
  evidence: ["profile:selected-practices", "investigation:findings", "tests:unit"]
  practices:
    tidy_first:
      status: not_applicable
      rationale: "Investigation mode, no production_code work product — the spike crate is throwaway evidence ADV-STORE-002 supersedes, and there was no existing code to prepare."
    tdd:
      status: not_applicable
      rationale: "Investigation mode, no production_code work product. The hashing-embedder implementation was written before its tests; the tests assert observed retrieval behaviour (a real finding, including one iteration where the first query wording had to change because it genuinely failed), not a red phase ahead of an implementation. No tdd:red-green is claimed."
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software]
  work_products: []
  status: complete
---

## Objective

Experiment to close the Solution Intent §9 open question: embedding provider
(local model vs. API) and placement (gateway on write vs. curator batch).
Compare candidates on retrieval quality over memory-sized texts, latency,
cost, and offline behavior, using throwaway spike code.

## Outcome

A recommendation is recorded in
[docs/tech-direction/embeddings.md](../../../../docs/tech-direction/embeddings.md):
a local pretrained model, embedded synchronously at gateway-write time, with
API-based embedding as an explicit non-default opt-in and re-embedding owned
by `totem-curator` as a batch job triggered by model-version changes.

**The recommendation is only partially evidence-backed.** Of the three
candidates in scope, only one — a purely lexical local hashing-trick baseline
— could actually be run and measured. This sandbox's egress policy blocks
every embedding-API host tested (`api.openai.com`, `api.cohere.com`,
`api.voyageai.com`, `huggingface.co`) with `Proxy failed to connect`, the
same class of block ADV-STORE-004 found for `download.surrealdb.com`. That
also blocks the local pretrained-model candidate (`fastembed`), which
downloads its model weights from a hub on first use. So the recommendation
rests on one real local baseline, published specs for the other two
candidates (via web search, not independently verified), and the general
argument in the findings doc — not on a head-to-head quality measurement.
This gap should be closed in an environment that can reach those hosts
before ADV-STORE-002 finalizes the choice.

No production code lands; `crates/totem-embedding-spike` is throwaway and
has no dependents.

## Planned Work

- [x] branch: create or confirm feature branch for this advance
- [x] define comparison criteria (quality on memory-sized texts, latency, cost, offline)
- [x] spike: run 2–3 candidate providers against sample memory texts — one
      executed (local hashing trick), one attempted and confirmed
      environment-blocked (API-based), one documented but not attempted
      because the same block applies (local pretrained model)
- [x] write findings + recommendation to docs/tech-direction/embeddings.md

## Bug Fixes

- [ ] None — no defect in existing repo code. The first version of the
      "shares vocabulary" query (`"What package manager should I use for JS
      work?"`) genuinely failed against its own precondition — it didn't
      share enough trigram substrings with its target — which is a fixture
      calibration issue caught by running the test, not a defect fixed in
      anything already written. See Changes Made.

## Risk + Rollback

- Risk (realised, contained): the egress block could have silently produced
  an untested "recommendation" if the API/pretrained-model candidates had
  simply been described rather than actually attempted. They were attempted
  (`probe_api_reachability`) and the block is captured as real output, not
  assumed.
- Risk (open): **the recommendation's quality comparison is incomplete.**
  Only the lexical baseline has measured retrieval numbers; the local
  pretrained-model and API candidates are unmeasured here. Carried forward
  to ADV-STORE-002, which should either run the equivalent harness against
  `fastembed` in an environment that can download its model, or explicitly
  accept the risk.
- Risk (open, minor): cost figures for the API candidate are quoted from a
  live web search of pricing-aggregator sites, not from a vendor's own
  pricing page or a measured call — flagged as unverified in the findings
  doc, not presented as measured.
- Rollback: findings only. `crates/totem-embedding-spike` is an isolated
  workspace member with no dependents; deleting it and its workspace entry
  removes the whole change. `totem-store`/`totem-gateway` will implement
  their own embedding integration rather than importing this spike.

## Evidence

- [x] profile:selected-practices — investigation mode, empty `work_products`;
      `tidy_first` and `tdd` recorded `not_applicable` with rationale in
      frontmatter. No `tdd:red-green` claimed.
- [x] investigation:findings —
      docs/tech-direction/embeddings.md, EMB-001…EMB-003, each tied to an
      executed experiment, a confirmed environment block, or an
      explicitly-labelled secondary source.
- [x] tests:unit — `cargo test -p totem-embedding-spike`: 3/3 pass, offline,
      deterministic, no network call.
- Negative control (sensitivity, in place of TDD's red phase): the
  paraphrased-query test
  (`hashing_embedder_loses_on_paraphrased_query_with_no_shared_vocabulary`)
  asserts the lexical baseline *fails* to rank its target first — proving
  the finding is a real limitation, not a cherry-picked pass.
- Reachability probe (`examples/probe_api_reachability.rs`): a control host
  known to be allowed (`index.crates.io`) succeeds; all four embedding-API
  hosts fail identically. Output captured verbatim in the findings doc.
- Not claimed: `ci:passed` — no pipeline result was observed for this branch.
- Not claimed: any API-provider quality, latency, or cost number as
  measured — all three are either unmeasured (candidates never reached) or
  explicitly flagged as unverified secondary sources.

## CI Evidence Notes

- Findings document is the primary artifact; no pipeline evidence expected.
- Checks run locally on the branch: `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `arrive doctor artifacts`, `arrive plan check`, `arrive check --strict`,
  `arrive score`.

## Reviewability

`arrive score` reports **68 [RED]** (size 43, novelty 20, risk 5) against
`origin/master`. The change is kept whole rather than split.

The `concurrency` risk flag is a **heuristic false positive**: nothing in
this advance touches threads, `async`, or shared state. It fires on the
words "single-threaded" and "no-Tokio" in `embed_corpus.rs`'s print string
and `embeddings.md`'s description of the `fastembed` crate — prose *about*
the absence of concurrency, not concurrency itself. Noted here rather than
disputed silently, per the honesty rule on correcting factual issues found
during implementation; no frontmatter field exists to override a scorer
heuristic, so `risk_flags` is left empty and this note stands as the
correction.

Of 865 changed lines, only 66 are the generated `Cargo.lock` delta for one
new pinned dependency (`ureq`) — small relative to the size score's real
driver, which is genuinely hand-written:

| File | Lines | Splittable? |
|---|---|---|
| `crates/totem-embedding-spike/src/lib.rs` | 254 | No — corpus, queries, provider trait, and the one executed candidate share one fixture; splitting the corpus from the candidate leaves neither reviewable alone |
| `docs/tech-direction/embeddings.md` | 181 | No — the findings, including the confirmed environment block, are the deliverable this advance exists to produce |
| `arrive/.../ADV-STORE-003.md` | 183 | No — this record, required by the same advance |
| `crates/totem-embedding-spike/tests/hashing_quality.rs` | 73 | No — assertions over the shared corpus fixture |
| `crates/totem-embedding-spike/examples/*.rs` | 103 | No — the two commands whose captured output the findings doc quotes verbatim |
| `Cargo.toml`, `Cargo.lock` | 24 | No — one workspace-member addition, one pinned dependency |

Splitting by file would land, for example, the corpus and hashing embedder
with no test proving the vocabulary-overlap finding, or the findings
document with no code a reviewer could run to reproduce EMB-001/EMB-002 —
each piece reviewable in isolation but none able to answer the question
this investigation exists to answer. A reviewer can read
`docs/tech-direction/embeddings.md` first and treat the crate as its
appendix, the same reading order ADV-STORE-004 recommended.

## Changes Made

### 2026-08-05 - feat: add the ADV-STORE-003 embedding investigation spike
- Cargo.toml: added `crates/totem-embedding-spike` as a workspace member
- Cargo.lock: regenerated for `ureq =2.10.1` (exact pin) and its transitive
  TLS/HTTP dependencies
- crates/totem-embedding-spike/Cargo.toml: pinned `ureq =2.10.1`, used only
  by the manual reachability probe
- crates/totem-embedding-spike/src/lib.rs: 10-text memory-shaped corpus
  across Knowledge/Instructions/Context categories, 5 hand-labeled queries
  (4 lexical-overlap, 1 deliberate paraphrase), the `EmbeddingProvider`
  trait, `HashingEmbedder` (character-trigram hashing trick, L2-normalized,
  FNV-1a, no external hash dependency), cosine similarity, and
  `evaluate_retrieval`
- crates/totem-embedding-spike/tests/hashing_quality.rs: three tests —
  lexical-overlap queries rank their target first; the paraphrase query does
  not (negative control); embeddings are unit-length
- crates/totem-embedding-spike/examples/embed_corpus.rs: prints the
  retrieval readout and per-call latency, quoted in the findings doc
- crates/totem-embedding-spike/examples/probe_api_reachability.rs: attempts
  a real HTTPS GET (honoring `HTTPS_PROXY`) to four embedding-API hosts plus
  one control host; not part of `cargo test --workspace` — like
  `totem-store-spike`'s `server-parity` feature, a default test run must
  never depend on live network conditions
- docs/tech-direction/embeddings.md: new — verdict, EMB-001…EMB-003 with
  captured command output, a candidate-comparison table stating plainly
  which cells are measured versus unverified, the provider/placement
  recommendation, and residual risk handed to ADV-STORE-002

  During this commit's development, the first version of the
  package-manager query ("What package manager should I use for JS work?")
  failed `hashing_embedder_wins_on_lexical_overlap_queries` for real (top-1
  came back wrong, rank 9 of 10) — it didn't actually share enough trigram
  substrings with its target text. Reworded pre-commit to "Should JavaScript
  projects use npm or pnpm?" and re-verified passing before this commit, so
  the committed "wins on shared vocabulary" finding reflects a query that
  actually shares vocabulary. Not split into a separate commit because the
  failing version was never itself committed.

### 2026-08-05 - docs: complete ADV-STORE-003
- arrive/systems/058-totem-core/advances/ADV-STORE-003.md: status complete,
  evidence, practice dispositions, refreshed CFU
- arrive/implementation-plan.yaml: plan item ADV-STORE-003 set to done

## Check for Understanding

1. `HashingEmbedder` ranks the paraphrased query
   (`"How do we keep private data out of shared scopes?"`) second, not last,
   against its target (`rank_of_expected=Some(2)`). Why does EMB-001 still
   call this a real limitation rather than a near-miss, and what would need
   to be true of a *real* agent recall query for "second place" to matter?
2. `probe_api_reachability.rs` treats `https://index.crates.io/config.json`
   as a control host, not just another candidate. What would the finding in
   EMB-002 mean — or fail to mean — if that control request had also failed?
3. The advance recommends a local pretrained model over the one candidate
   that was actually measured (the hashing trick). Point to the specific
   evidence item in `docs/tech-direction/embeddings.md` that the
   recommendation rests on for that candidate, and say plainly what kind of
   evidence it is (measured vs. sourced vs. argued).
4. `Cargo.lock`'s `ureq` pin is `=2.10.1`. If a future run of
   `probe_api_reachability` needs a newer `ureq` for an unrelated reason,
   what in this advance's reasoning (see `dev-practices.md`'s SurrealDB pin
   precedent) would break if the pin were loosened to `"2"` instead?
5. The Changes Made log describes a query that failed
   (`hashing_embedder_wins_on_lexical_overlap_queries`, rank 9 of 10) and
   was reworded before the commit that includes it. Why does that pre-commit
   failure still count as real evidence for the "wins on shared vocabulary"
   finding, given it never has its own commit in `git log`?
6. `frontmatter.practices.tdd` is `not_applicable`, yet
   `tests/hashing_quality.rs` was written and iterated against real
   assertion failures. What distinguishes this from `tdd:red-green`, and
   where in the frontmatter is that distinction recorded?
7. Solution Intent §5 assigns `totem-curator` "deduplication, consolidation,
   decay processing, and contradiction detection" — not embedding. What
   specific argument in §4 of the findings doc justifies giving curator the
   *re-embedding* job anyway, and what would break if gateway owned it
   instead?
