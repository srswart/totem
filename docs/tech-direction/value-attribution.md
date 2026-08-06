# Tech Direction: Value-attribution proxy experiment

**Status:** Findings accepted, provisional (small-n, re-run trigger below) ·
**Date:** 2026-08-06 · **Advance:** ADV-CORE-004 · **Companion:**
[solution-intent.md](../solution-intent.md) §9,
[project-brief.md](../project-brief.md) ("Value signal quality") · to be
implemented by `ADV-CORE-002` (currently `status: planned`)

Solution Intent §9 leaves open "value attribution depth: how far to chase
'did this memory change the outcome?' beyond citation signals in v1." The
project brief names this a key risk directly: "usage metering is easy, value
attribution is hard; start with simple proxies (retrieval → citation →
outcome) and iterate." This document scores those proxies against a labeled
corpus and records the v1 signal set and per-category weights ADV-CORE-002
should implement.

**Verdict: lead v1 `value_score` with the citation signal, add
`provenance.derived_from` graph linkage as a high-precision booster once the
gateway exposes it (it currently does not — VAL-004), and do not weight raw
retrieval/`use_count` into `value_score` directly.** Outcome-linkage — did
the consuming work actually behave as the memory said it should — is the
best predictor in this corpus (precision 1.0, recall 0.92) but requires
reading code, not text, so it cannot run as a cheap per-turn signal today;
citation is the pragmatic, cheap stand-in (precision 0.80, recall 0.67).
Explicit feedback
(`totem_feedback`) has zero data points — the tool does not exist yet
(`ADV-GATEWAY-004`) — and is deferred, not weighted, in v1.

## 1. What was run

No cargo tests: this is a documentation/analysis investigation, not a spike
crate (`work_products: []`, mode `investigation`; no `production_code`
touched). The method was retrospective, direct inspection of this repo's own
history — the only real corpus available before any agent has used
`totem_recall` against its own findings in production.

| | |
|---|---|
| Corpus | 17 (finding → consuming-advance) pairs, drawn from this repo's own `docs/tech-direction/*.md` findings (TD-001…TD-011, EMB-001…EMB-004, MCP-001…MCP-005) and the advances/code that could have consumed each |
| Labeling | Direct code inspection (`grep`/`Read` of `crates/totem-store/src`, `crates/totem-gateway/src`, `crates/totem-core/src`) plus the advance files and journey reports, cross-checked against `implementation_completed_at` timestamps so every "consuming" advance genuinely postdates its finding |
| Scored by | One author (this run) — not blinded; see §5 caveats |

## 2. Method — why a real corpus instead of synthetic sessions

The advance's Planned Work asks for "sample sessions with known 'memory
actually mattered' labels." Two options existed: hand-write synthetic
sessions, or use this repo's own real development history, where later
advances demonstrably did or did not draw on earlier recorded findings. The
second was chosen because it is real, checkable evidence (I can read the
actual code and confirm compliance or its absence) rather than an invented
scenario calibrated to whatever proxy I expected to win. The tradeoff, held
honestly: n=17, single-repo, single-project, and not blinded — the same
person who read the findings labeled the ground truth. This is exactly the
risk the advance file already names ("hand-labeled samples bias the proxy
choice; treat the v1 set as provisional").

For each pair, four candidate signals were scored, each mapped to a concrete
mechanism in Totem's actual schema (`totem-core`) or its absence:

- **Retrieval (R)** — was the finding available before the consuming work
  began? In production this would be "does an `AccessLogEntry` with
  `operation: Recall` exist for this memory in the session" — but no such
  telemetry exists yet (no agent has run `totem_recall` against this repo's
  own findings during real development). Reconstructed here as "did the
  finding's file exist, with `implementation_completed_at` before the
  consuming advance's own `implementation_completed_at`" — an availability
  proxy, not a logged recall event.
- **Citation (C)** — does the consuming artifact (code comment, doc, advance
  body) cite the finding's own ID (`TD-XXX`/`EMB-XXX`/`MCP-XXX`) verbatim.
  Maps to a signal Totem could log directly: did the agent's saved output
  reference a specific recalled memory's id.
- **Outcome-linkage (O)** — does the consuming artifact's actual delivered
  *behavior* implement or honor the finding's stated constraint, checked by
  reading the code, independent of whether it cites the finding at all. Maps
  to `provenance.derived_from` — a memory's graph link back to the memory
  that informed it (already in the schema, `crates/totem-core/src/provenance.rs`,
  `DEFINE FIELD provenance.derived_from ... TYPE array<record<memory>>` in
  `crates/totem-store/src/schema.rs:73`).
- **Explicit feedback (F)** — an accept/reject/rating signal from the
  consuming actor (`totem_feedback`). **Zero data points**: the tool does
  not exist yet (`ADV-GATEWAY-004`, `status: planned`). Not scored below.

**Ground truth ("mattered", M)** — my own judgment, from reading the actual
code, of whether ignoring or violating the finding would have caused a
defect (wrong behavior, a security gap, or wasted correctness effort) in the
consuming advance's delivered outcome. This is a judgment call, not a
measurement — see §5.

## 3. The dataset

| # | Finding | Consuming context | R | C | O | M (mattered) | Note |
|---|---|---|---|---|---|---|---|
| 1 | TD-002 (`<\|K,EF\|>` uses the index) | ADV-STORE-001, `memory.rs` recall query | Y | Y | Y | **Y** | Cited by id at `memory.rs:288`; code uses the numeric-`ef` form and asserts on `EXPLAIN FULL` |
| 2 | TD-003 (scope predicate pushed into index scan) | ADV-STORE-001, `memory.rs::statement()` | Y | Y | Y | **Y** | Store-generated `scope IN $scopes`, never caller-supplied |
| 3 | TD-004 (typed `Datetime`, never a string) | ADV-STORE-001, `row.rs::instant()` + regression test | Y | Y | Y | **Y** | `row.rs:410` names TD-004 directly in the test that pins the behaviour |
| 4 | TD-005 (`vector::distance::cosine` not callable) | ADV-STORE-001, `memory.rs` recall query | Y | **N** | Y | **Y** | Code correctly uses `vector::distance::knn()`, never the non-callable function — but no comment cites "TD-005" anywhere in `totem-store` |
| 5 | TD-006 (transactions atomic across writes) | ADV-ARRIVE-SYNC-001, `landscape.rs` ingestion | Y | Y | Y | **Y** | `landscape.rs:265` cites TD-006 for why a partial ingest write can't leave the landscape |
| 6 | TD-008 (live notifications unordered within a transaction) | ADV-STORE-001 (recorded, deferred) | Y | Y | N/A | **N** | Advance file's own risk table names TD-008 explicitly, then correctly defers it — no live-query consumer exists in this advance |
| 7 | TD-009 (live queries refused over HTTP) | ADV-STORE-001, `store.rs:38` (forward note) | Y | Y | N/A | **N** | Cited as a forward pointer for the console; no live-query code exists yet to be right or wrong about |
| 8 | TD-010 (server denies scripting/network by default) | none yet (deployment topology deferred, `ADV-INFRA-001` reserved) | Y | N | N/A | **N** | Available in the doc, correctly uncited — nothing consumes it yet |
| 9 | TD-011 (restricted DB user drops writes silently) | ADV-STORE-001, `store.rs:24`, `lib.rs:21` | Y | Y | Y | **Y** | Store runs as the fully-privileged system user per the finding's stated tradeoff |
| 10 | EMB-001 (lexical baseline measured geometry) | ADV-STORE-002, `embedding.rs` test embedder | Y | Y | Y | Y (moderate) | Shapes the deterministic test double's design rationale |
| 11 | EMB-002/EMB-003 (API/model download blocked in-sandbox) | ADV-STORE-002, `embedding.rs` feature gate | Y | Y | Y | **Y** | `fastembed` is an off-by-default cargo feature, exactly the shape the finding required |
| 12 | EMB-004 (BGE-small-en-v1.5, 384 dims, pinned) | ADV-STORE-002, `schema.rs`, `fastembed_embedder.rs` | Y | Y | Y | **Y** | `EmbeddingModel::BGESmallENV15`, `DIMENSION 384` — exact pin match |
| 13 | MCP-001 secondary (set `serverInfo.name` explicitly, not the macro default) | ADV-GATEWAY-002, `mcp.rs` | Y | **N** | **N** | **Y** | The recommendation was concrete and actionable; `#[tool_router(server_handler)]` is used with no `get_info`/name override — a genuine retrieval-and-apply miss, not a proxy-scoring gap |
| 14 | MCP-002 (`rmcp` mature, pin `=3.1.0`) | ADV-GATEWAY-002, `Cargo.toml` | Y | weak | Y | **Y** | Comment cites the *source advance* (`ADV-GATEWAY-005`), never the finding id "MCP-002" itself; pin matches exactly |
| 15 | MCP-003 (Claude Code prefers streamable HTTP; stdio for desktop) | ADV-GATEWAY-002, `bin/mcp_stdio.rs` | Y | N | Y | **Y** | Binary implements exactly the desktop-local stdio path the matrix recommends; no citation by id |
| 16 | MCP-004 (Claude API connector: HTTP/SSE only, tools-only) | none yet (`ADV-GATEWAY-003` not built) | Y | N | N/A | **N** | Correctly unconsumed — its advance hasn't started |
| 17 | MCP-005 (Cursor reach unverified) | none yet (`ADV-GATEWAY-003` not built) | Y | N | N/A | **N** | Same — correctly unconsumed |

12 of 17 pairs are ground-truth "mattered"; 5 are ground-truth "not yet" (a
finding correctly recorded and either not yet due, or — once, #13 — actually
missed).

## 4. Findings

### VAL-001 — Outcome-linkage has the highest precision in this corpus, but only because it is read from code, not text. *(confirmed on this corpus, small-n)*

Predicting "mattered" wherever O=Y: 11 predictions, all 11 ground-truth
positive (precision 1.0), missing only #13 (MCP-001, recall 11/12 = 0.92 —
the one real miss, where the recommendation was never applied at all, so
there was no behavior to link). Outcome-linkage is the closest proxy to "did
this memory change the outcome" because it *is* that question, checked
directly — but computing it required reading `totem-store`/`totem-gateway`
source by hand. It is not something a gateway can compute cheaply per turn
today.

### VAL-002 — Citation is cheap but imperfect in both directions. *(confirmed on this corpus, small-n)*

Predicting "mattered" wherever C=Y (weak citations, #14, counted as cited):
10 predictions, 8 true positive, 2 false positive (#6 TD-008, #7 TD-009 —
correctly cited as forward notes, not yet delivering value), for precision
0.80. Recall is 8/12 = 0.67: citation misses #4 (TD-005), #13 (MCP-001), #14
strictly (MCP-002's finding id itself was never cited, only the advance id),
and #15 (MCP-003) — all cases where the code is correct but nobody wrote the
finding's id down. **Citation without use** (TD-008/TD-009) and **use
without citation** (TD-005/MCP-002/MCP-003) are both real, and roughly
balanced in this corpus — exactly the two failure modes the advance's
Outcome section asked this investigation to name.

### VAL-003 — Retrieval alone has no discriminating power. *(confirmed on this corpus, small-n)*

R=Y for all 17 pairs (every finding was available before its candidate
consumer existed), so predicting "mattered" from retrieval alone gets
perfect recall (12/12) at the cost of precision 0.71 (12/17) — it "predicts"
every not-yet-due finding as mattering too. This is the brief's own
"metering is easy, attribution is hard" risk, reproduced numerically: being
recallable is necessary but carries no information about whether the recall
was used.

### VAL-004 — The schema already carries the best signal; the gateway API does not expose it yet. *(confirmed by code inspection)*

`provenance.derived_from` (`crates/totem-core/src/provenance.rs`, builder
`.derived_from(sources)`) and its store column
(`crates/totem-store/src/schema.rs:73`,
`DEFINE FIELD provenance.derived_from ON memory TYPE array<record<memory>>`)
already exist — but `SaveRequest` (`crates/totem-gateway/src/dto.rs`) and
`totem_save`'s MCP parameters (`crates/totem-gateway/src/mcp.rs`) expose no
`derived_from` field at all. A caller cannot set it through either surface
Totem actually ships today. This investigation's O=Y labels were read
retrospectively from code behavior, not from any `derived_from` link an
agent actually wrote — so VAL-001's precision describes what *could* be
measured once this gap closes, not what Totem measures today.

### VAL-005 — Explicit feedback is unscoreable: zero data points. *(not executed — no tool exists)*

`totem_feedback` is `ADV-GATEWAY-004`, still `status: planned`. No entry in
this corpus has a feedback event to score. Carried forward as a deferred
signal, not a rejected one.

## 5. Recommendation for ADV-CORE-002

**v1 signal set:**

1. **Citation-in-following-turn** — the primary, always-on signal. Cheapest
   to compute (does the next turn's saved output reference the recalled
   memory's id), positive in this corpus 0.80 of the time it fires.
2. **`provenance.derived_from` graph linkage** — the highest-precision
   signal (VAL-001), but currently unmeasurable in production (VAL-004).
   **Before ADV-CORE-002 can use it, the gateway's `SaveRequest` and
   `totem_save` MCP tool must add a `derived_from` field** so a writing
   agent can record which recalled memories informed the new one. Until
   then, weight it at zero and fall back to citation alone; once populated,
   it should outweigh citation per-occurrence (it is sparser but far more
   precise).
3. **`use_count` / retrieval** — floor signal only. Per VAL-003 it must
   **not** be weighted into `value_score` directly; it already has its own
   field in `Economics` separate from `value_score`, which is the right
   shape — feed it into `currency`/freshness, not into value.
4. **Explicit feedback (`totem_feedback`)** — **not part of v1 weights**; no
   data exists (VAL-005). Re-open this experiment once `ADV-GATEWAY-004`
   ships and real feedback events accumulate (trigger: enough sessions using
   `totem_feedback` to label a comparable-sized corpus, e.g. ≥17 real
   feedback events across categories).

**Per-category weights:** this corpus is entirely **Knowledge**-shaped
(measured technical findings) and partly **Instructions**-shaped (binding
constraints — e.g. "the store must generate the scope predicate, never the
caller" reads exactly like a standing rule). For those two categories, the
citation + derived_from scheme above applies directly. For the other four
categories, this investigation has no data and says so plainly rather than
guessing:

- **Context** — short `default_ttl` (12h, `category.rs`) means citation
  signal has little time to accumulate before expiry; weight retrieval/
  freshness only for this category, not citation.
- **Episodic** — append-only audit substrate, never ranked by `value_score`
  at all (`category.rs`'s `injection_priority`/`decays: false` already
  treat it specially); value-attribution is inapplicable here by design, not
  by omission.
- **Identity, Uncertainty** — no examples arose naturally in this repo's own
  findings corpus. Provisionally treat like Knowledge until real data exists
  (Uncertainty in particular should be revisited once `ADV-CONSOLE-002`'s
  queue produces real contest/resolution sessions).

## 6. What this investigation deliberately did not answer

- Real production precision/recall from live `AccessLogEntry` data — none
  exists yet; every number above is retrospective.
- Any Context, Uncertainty, or Identity category example — none arose in
  this repo's own findings corpus (see §5).
- The decay/weighting math itself (`relevance × value × currency`) —
  `ADV-CORE-002`'s job, this document only orders and scopes the inputs.
- Blinded labeling — one author both read the corpus and scored it; a second
  independent labeler was not available in this run.
- Whether `totem_feedback`, once it exists, will actually agree with the
  citation/derived_from proxies — VAL-005 is a placeholder, not a finding.

## Evidence

- Direct code inspection: `crates/totem-store/src/{memory,row,schema,store,embedding}.rs`,
  `crates/totem-store/src/embedding/fastembed_embedder.rs`,
  `crates/totem-gateway/src/{dto,mcp,handlers}.rs`,
  `crates/totem-core/src/{provenance,category}.rs` — grep and read, captured
  in the labeled dataset (§3).
- `arrive/systems/058-totem-core/advances/ADV-STORE-001.md`'s own risk
  table, cross-checked for #6/#7 (TD-008/TD-009) against the source code.
- Cross-checked ordering: every "consuming" advance's
  `implementation_completed_at` postdates its finding's, confirmed by
  reading each advance file's frontmatter directly (§1).
- Not claimed: any measurement from live agent telemetry — none exists in
  this repo yet (VAL-004/VAL-005).
