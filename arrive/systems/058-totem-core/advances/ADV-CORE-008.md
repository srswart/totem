---
advance:
  id: "ADV-CORE-008"
  title: "Recall ranks by what was asked: bound the value loop's grip on relevance"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store"]
  started_at: "2026-08-08T04:45:00Z"
  implementation_completed_at: "2026-08-08T07:00:00Z"
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 34
  risk_flags: ["behaviour_change"]
  evidence: ["tdd:red-green", "tests:unit", "golden-queries"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

**Recall currently ignores the query.** Four questions asked of the deployed
gateway — two of them verbatim copies of a stored record's own body —
returned the same seven records in the same order. A memory system that
answers the same thing regardless of what was asked is not a memory system.

This was found taking ADV-STORE-008's golden-query evidence, immediately
after the real embedder went into the deployment. It is **not** an
ADV-STORE-008 regression: a test there proves ranking survives the re-embed
pass. It is pre-existing, and the deterministic embedder concealed it —
nobody trusted those rankings, so nobody looked at them.

## The mechanism

`crates/totem-core/src/scoring.rs`:

```
combined_score = relevance * value_score * currency * category_weight
```

**Corrected 2026-08-08, before implementation.** The first draft of this
advance said the loop runs through `value_score` and that
`reinforce_usage` raises it. **That is wrong.** `reinforce_usage` sets
`use_count += 1`, `last_used_at`, and `currency = 1.0`; it never touches
`value_score`, which moves only via `CITATION_BOOST` (+0.2) when a new memory
cites an existing one. The corrected mechanism, measured rather than reasoned
about:

| record | value_score | currency | category_weight | product of the three |
|---|---|---|---|---|
| Instructions | 1.000 | 1.000 (never decays) | **1.000** | **1.000** |
| Knowledge | 1.000 | 0.863 (14-day half-life) | **0.500** | **0.432** |

- `relevance_from_distance(d) = 1 / (1 + d)`. Cosine distance runs `[0, 2]`,
  so relevance runs `[0.33, 1.0]` — **a 3x range across every possible
  query**, from an exact match to a semantic opposite.
- The three non-relevance factors give an `Instructions` record a **2.3x
  structural advantage** over a `Knowledge` one before any query is asked:
  `category_weight` is 1.0 against 0.5, and `Instructions` is one of the
  categories whose lifecycle never decays, so its currency stays at 1.0 while
  `Knowledge` sinks.

An exact body match on a Knowledge record therefore scores about
`1.0 x 0.432 = 0.432`, and an *unrelated* Instructions record scores about
`0.45 x 1.0 = 0.45`, and wins. Relevance's entire dynamic range is not enough
to overcome a category difference.

**Where the feedback loop actually is.** `reinforce_usage` resets
`currency = 1.0` on every record a recall returns, so a recalled record stays
current while unrecalled ones decay. Whatever ranks highly keeps its currency
pinned and ranks highly more often. On the deployment — where one
`instructions` memory had been returned by every recall for days — that
compounds with the structural advantage above until nothing else can place
first.

**Why `value_score` still needs bounding even though it is not today's
cause.** `CITATION_BOOST` adds 0.2 per citation with **no ceiling**, so
`value_score` is genuinely unbounded; it simply has not grown yet, because
this estate has few derived memories. Saturating it now prevents the same
class of failure arriving later through a different term, and the fix should
not wait for it to bite.

Reproduced in `crates/totem-store/tests/recall_ranking.rs`, `#[ignore]`d so
the suite stays green while the defect stays executable:

```
cargo test -p totem-store --test recall_ranking -- --ignored
```

## This is a product decision, not only a bug

The value loop is deliberate (ADV-CORE-002, `docs/solution-intent.md` §4).
A memory that has proven useful *should* carry weight — that is much of what
distinguishes Totem from a vector index. The defect is not that history
counts; it is that **history is unbounded while relevance is bounded**, so
history always wins in the limit.

Whoever implements this must answer: *how much may a memory's history
outweigh what was actually asked?* Some candidate shapes, none yet chosen:

1. **Bound `value_score`'s contribution** — saturate it (e.g. `log`, or a
   ceiling), so a well-used memory gets a strong but finite advantage.
2. **Widen relevance's range** — a sharper distance transform, so an exact
   match is worth far more than 3x a poor one.
3. **Make relevance a gate rather than a factor** — below a distance
   threshold a record does not compete at all, whatever its history.
4. **Damp the reinforcement** — a recall that the caller never used should
   not count the same as one that fed a decision (`totem_feedback` already
   carries that signal and is currently not consulted here).

**Decided by Shawn, 2026-08-08: (3) + (1) — a relevance gate combined with a
saturating value score.** The measurement above, taken after that decision,
does not overturn it but does change what each half is *for*:

- **The gate (3) is what fixes the observed failure.** An unrelated
  `Instructions` record whose distance falls below the threshold stops
  competing entirely, and no structural advantage can save it. This is the
  half that makes the failing test pass.
- **Saturating `value_score` (1) is prophylactic, not curative.**
  `value_score` is 1.000 on every record in the estate today, so bounding it
  changes nothing now — but `CITATION_BOOST` is unbounded and would
  eventually reproduce this failure through a different term. Kept, and
  recorded as guarding a future failure rather than fixing the present one,
  so nobody later mistakes it for the thing that worked.
- **A third lever was to be left out, and measurement forced it in.** The
  plan of record said `category_weight` would not be touched: whether an
  `instructions` memory should carry a 2.3x head start reads as a question
  about `docs/solution-intent.md` §2.1's injection priorities rather than
  about ranking mechanics. **The gate alone did not fix the defect.** The
  unrelated `Instructions` record sits at cosine distance **0.824** — inside
  the gate, which is at orthogonality — and still won, 0.548 against the
  exact match's 0.432. The gate never fired, because the record was not
  irrelevant enough to be excluded and category was enough to carry it.

  The underlying mistake was narrower and more interesting than "the weights
  are wrong": `injection_priority` was doing two different jobs. It answers
  *"if I am assembling a context window, what goes in first?"* — a budget
  question, where a 10x spread is reasonable — and was being reused to answer
  *"which of these did the caller mean?"*. Category **order** is preserved
  exactly; only its magnitude is compressed. Agreed with Shawn after the
  measurement, 2026-08-08.

**Corrected again during implementation: the bound governs the product, and
`currency` was never exempt.** Two further defects, both found by the corpus
economics fixtures on their first run — against the *already-fixed* ranker:

1. **`currency` was waved through** on the reasoning that it "is already
   `[0, 1]`" and therefore could not outweigh anything. That reasoning is
   wrong: bounding a factor's *magnitude* says nothing about the *ratio*
   between two records. At the 14-day half-life, a memory read yesterday
   carries **~95x** a memory nobody has opened for three months. An exact
   answer written last month loses to a tangentially-related note read this
   morning — the same defect, through the one term nobody checked.
2. **The rule was stated per-factor, but `combined_score` multiplies.** Three
   factors each "safely" bounded at 2x compose to **8x**, so history could
   still outweigh relevance by 4x. Each factor now holds a geometric third of
   the budget, `relevance_range^(1/3)` ≈ 1.26, so the product is exactly 2x.

Only the *upward* direction is bounded. A `value_score` below its 1.0 default
is a deliberate demotion — negative feedback, or retirement to zero — and
keeps its full force. The budget governs advantage a record accrues on its
own, not a judgement somebody made about it on purpose.

Below a distance threshold a record does not compete at all, whatever its
history; above it, `value_score` contributes on a saturating curve so a
well-used memory carries a strong but *finite* advantage.

Why this pair rather than either alone: the gate answers the failure directly
— an irrelevant record cannot win no matter how much history it has
accumulated — while saturation stops the runaway that produced this in the
first place, without discarding the value loop. Widening relevance's range
(2) alone would only move the crossover point; the loop would still win
eventually, just later. Damping reinforcement (4) is worth doing and is
recorded as out of scope: `totem_feedback` already carries a
did-this-help signal that reinforcement ignores, and wiring it in is a
separate change with its own evidence.

Two numbers must be chosen and justified in the record rather than tuned
until the tests pass: the gate's distance threshold, and the saturation
point. Both are product decisions in miniature — the threshold decides what
"not relevant enough to consider" means, and the saturation point decides how
much history is enough.

### The two numbers, and why only one of them is a choice

**The gate: cosine distance 1.0.** The store's index is `DIST COSINE`, so
distance is `1 − cos θ` over `[0, 2]`, and **1.0 is exactly orthogonality** —
a record at this distance shares no direction with the query at all, and past
it the two are negatively correlated. The gate is therefore not a tuning
parameter but a statement with a meaning: *a record must be positively
similar to what was asked, not merely less dissimilar than its neighbours.*
This is the only genuinely free choice in the advance, and it was made on
what the metric means rather than on what made a test pass.

**The saturation point: derived, not chosen.** Given the gate, everything
else follows arithmetically:

```text
relevance among competing records = 1 / (1/(1+gate)) = 1 + gate = 2.0
per-factor budget                 = 2.0 ^ (1/3)      ≈ 1.26
non-relevance floor               = 1 / 1.26         ≈ 0.794
```

The 3 is the number of non-relevance factors in `combined_score` (value,
currency, category), and the root is geometric because they multiply. Written
as a derivation in code — `relevance_range()`, `per_factor_range()`,
`non_relevance_floor()`, `value_saturation_ceiling()` — so moving the gate
moves all of them together and the claim cannot quietly stop holding. A
hand-picked ceiling of 3.0 would have let history outweigh relevance by 1.5x
even in the single-factor case: the very failure this advance exists to fix,
smaller and harder to see.

## Behavioral Change

After this advance, a query whose text exactly matches a stored record ranks
that record first, regardless of any other record's accumulated history — and
the paraphrase queries EMB-004 measured behave semantically on the deployed
instance.

What must **not** change: a well-used memory still outranks an unused one
when relevance is comparable. If this advance turns Totem into a plain vector
index it has overshot, and the tests should say so in both directions.

## Planned Implementation Tasks

- [x] branch / claim
- [x] decide the shape with Shawn — relevance gate + saturating value score,
      reasoning and rejected alternatives recorded above
- [x] choose and justify the two constants (gate threshold, saturation point)
      from the corpus, not by tuning until tests pass — see "The two numbers"
      above; only the gate is a choice, the rest is derived from it
- [x] test: an exact body match outranks an unrelated high-`value_score`
      record (was `#[ignore]`d in `recall_ranking.rs` — now un-ignored and
      passing)
- [x] test: the converse — comparable relevance, and the well-used record
      still wins. The value loop must survive this advance.
- [x] **corpus economics** — `crates/totem-store/src/corpus.rs` never set
      `value_score` or `last_used_at`, so every golden record had identical
      pristine economics and `eval_quality` scored a perfect 1.0 against the
      broken system. Added the economics pair and the golden query between
      them. **It failed on its first run against the already-fixed ranker**
      and found the two further defects recorded above — which is the whole
      argument for the fixture.
- [x] feat: the scoring change
- [x] name the missed queries when `eval_quality` fails — it reported a bare
      `0.5` and left the reader to guess which of five queries produced it,
      in the one test whose purpose is to be read when something is wrong
- [x] golden queries on the deployed instance — the evidence ADV-STORE-008
      could not meet. Taken; see "Measured result" above. **Half the
      behavioural claim is met.** Recorded rather than rounded up.

## Closing disposition

Closed `complete` with the behavioural claim **partly** met, deliberately,
rather than held open. What shipped is sound and independently valuable — the
gate, the three bounds, the corpus economics fixtures, and an evaluation that
can now fail. What is unresolved is a *diagnosis*, and it cannot be made from
inside this advance: recall exposes no score, so there is no way to ask the
deployment why it ordered anything.

Holding CORE-008 open would block phase-012 and, behind it, the cutover
(ADV-INFRA-003), waiting on an answer no amount of further work here can
produce. The residue is handed to **ADV-GATEWAY-016**, which makes ranking
observable, and the question is then decided in one call rather than guessed
at.

**What the next advance must not assume.** The 1.13x figure says the *weights*
are unlikely to be the cause; it does not say the embedder is. Both remain
open until measured.

## Relationship to ADV-CORE-005

**ADV-CORE-005 is the evaluation advance for this defect.** It is not new and
it has never run: authored 2026-08-05, `planned`, `pr_links: []`. It is
scoped to exactly this question — *"does retrieval actually favor relevant,
valuable, current memory?"* — and its Quality Risks section named this hazard
in advance, before anyone had seen it.

The harness it needs also already exists: **ADV-GATEWAY-008** (done) built
the recall-quality scorer with precision@1 and expected-item rank;
**ADV-STORE-005** (done) built the golden corpus. No new tooling advance is
warranted, and authoring one would duplicate work already shipped.

What is missing is not tooling but a **fixture state**: the corpus never
varies `value_score`, so the scorer measures a system in a condition it is
never actually in and reports 1.0. Fixing that is a task in this advance,
because a fix that cannot be shown to have changed a number is not evidence
of anything.

Sequencing: this advance adds the economics fixtures and the scoring change;
CORE-005 then runs the full evaluation over the corrected corpus, measures
before and after, and dispositions whether the fix overshot.

## Scope and Boundaries

**In scope:** the scoring function, its tests, the corpus economics fixtures,
and the deployed golden-query evidence.

**Out of scope:** the embedder (ADV-STORE-008, done); whether
`totem_feedback` should feed reinforcement — worth doing, but it is a
separate change with its own evidence; recall latency; the full quality
evaluation and its dispositions (ADV-CORE-005).

## Risk + Rollback

- Risk (`behaviour_change`): every recall in the system changes order. There
  is no migration and no undo beyond reverting — the ranking is computed at
  read time, so a revert is complete and immediate.
- Risk: over-correcting into a plain vector index, discarding the value loop
  that is much of Totem's thesis. The converse test above exists to catch it,
  and it should be written first.
- Rollback: revert the branch; ranking returns to today's behaviour.

## Evidence

- [ ] tdd:red-green — the reproduction already exists and already fails;
      un-ignoring it is the red step, and it was written before any fix.
- [ ] tests:unit — both directions, relevance and history.
- [ ] golden-queries: executed on the deployed instance. This is the claim
      ADV-STORE-008 could not make, and the reason this advance exists.

### Measured result: half the behavioural claim is met, and half is not

Taken 2026-08-08, `scripts/golden-queries.sh`, six questions,
`evidence/golden-queries-{before,after}.txt`. Same seven records, same
embedder (`fastembed-bge-small-en-v1.5`, uniform), ranking the only variable.

**What the fix did:**

- **The near-verbatim query is fixed.** "The gateway owns the embedded store
  exclusively…" returned DEP-001 — the exact answer — at position **3**
  before and position **1** after.
- **The gate fires.** Every query returned 7 of 7 records before and 6 after:
  one record now fails the gate and is excluded rather than ranked. Recall
  finally declines to answer with something, which it had never done.

**What the fix did not do:**

- **Five of six queries still return the same `instructions` record first** —
  "Always run arrive plan check before pushing plan edits" — including
  "Which process is allowed to open the SurrealDB engine?", whose ordering is
  **completely unchanged**. That query is the flagship failure this advance
  was opened for, and it still fails.

**So the weights are probably no longer the cause.** With `value_score` at
1.0 everywhere and currency uniform (see below), the entire surviving
structural advantage of `Instructions` over `Knowledge` is `category_weight`:

```text
Instructions 1.0000 / Knowledge 0.8854 = 1.13x, against relevance's 2x
```

A 1.13x edge can only decide the ordering if relevance itself puts the two
records within 13% of each other — that is, if the embedder genuinely
considers a note about `arrive plan check` nearly as close to "which process
may open the engine?" as DEP-001 is. That is an **embedding or corpus**
question, not a ranking-weights one, and it is not what this advance set out
to fix.

**It cannot be confirmed from outside, and that is its own finding.** The
recall response carries no score and no distance, so there is no way to ask
the deployed system *why* it ordered something the way it did. This is the
same blind spot as ADV-STORE-008's — where the running embedder could not be
named until an endpoint was added for it — arriving one layer up. Ranking is
now the least observable part of the system.

**The before-run neutralized the currency fix before it could be tested.**
`reinforce_usage` set `currency = 1.0` on all seven records during the before
capture, so by the after capture every record carried identical currency and
that term cancelled everywhere. The comparison therefore measures the gate and
the category compression only. The currency bound is covered by unit tests and
by the corpus fixture, but it has **not** been exercised against the
deployment, and this record should not be read as saying it has. The hazard
was written down in this advance before the measurement was taken, and the
measurement walked into it anyway — the note was not specific enough to say
*take the before capture last, or on a copy*.

### Taking that evidence changes the thing being measured

`recall()` calls `reinforce_usage` on every record it returns
(`crates/totem-store/src/memory.rs:582`): `use_count += 1`,
`last_used_at = now`, `currency = 1.0`. **Reading the store mutates the
ranking inputs of whatever the read returned.** Measure the deployment twice
and the second measurement is biased toward whatever the first one surfaced —
which is not an incidental flaw in the method, it is the feedback loop this
advance is about, observed directly.

Two consequences for the golden-query evidence:

- **No `totem_save` or `totem_recall` against `totem-dev` between the before
  and after measurements**, for any reason — including the recall-first
  discipline in `CLAUDE.local.md`, which has to yield here. A memory saved
  mid-measurement also changes the population being ranked.
- **The before-measurement is already contaminated, in the conservative
  direction.** Every recall taken during ADV-STORE-008 pinned `currency = 1.0`
  on the incumbent that kept winning, strengthening exactly the record the fix
  has to dislodge. That biases *against* the fix appearing to work, so a
  demonstrated improvement is real; it is a floor on the effect, not an
  inflation of it. Worth stating plainly rather than presenting the before
  number as clean.

This is also a standing hazard for ADV-CORE-005, which will want to run the
same evaluation repeatedly. An evaluation that reinforces what it retrieves
cannot be run twice against the same store and compared. Either it needs a
non-reinforcing read path, or each run needs a fresh seeded store — and that
choice belongs to CORE-005, not here.

## Check for Understanding

1. `relevance` spans 3x; `value_score` is unbounded and is increased by the
   act of being recalled. State the loop in one sentence, and say why it
   converges on a single winner rather than oscillating.
2. Why did the deterministic embedder conceal this for weeks, when it was
   equally true then?
3. The advance insists a well-used memory must still beat an unused one at
   comparable relevance. Which failure is that guarding against, and why is
   the test for it worth writing *before* the fix?
4. Option (3) makes relevance a gate rather than a factor. What does a gate
   do that a multiplier cannot, and what does it risk when the query is
   vague?
5. This was found by asking a deployed system four questions, not by any
   test. What class of defect does that suggest the suite is structurally
   unable to reach here?
6. `eval_quality` asserts `precision_at_1 == 1.0` and passes today, against a
   system that ignores the query. What exactly is that assertion measuring,
   and what one property of the corpus makes it vacuous?
7. No new harness advance was authored for this. Which two completed advances
   already provide the tooling, and what does that suggest about reaching for
   a new advance when an evaluation comes back green?
8. `currency` was exempted from the bound because it is "already `[0, 1]`".
   That is true and it is not a defence. State the distinction the argument
   missed, in one sentence, and give the ratio two Knowledge records can
   reach on age alone at a 14-day half-life.
9. Each of the three non-relevance factors was bounded at relevance's 2x
   range, and history could still outweigh relevance by 4x. Why — and what
   does that imply about the bound if a fourth factor is ever added?
10. Both of those defects were found by a *fixture*, not by reasoning and not
    by a new test of the fix. What property did the corpus previously have
    that made them invisible, and why does that make "the evaluation passed"
    weak evidence in general?
11. Reading the store reinforces what it returns. Say what that does to a
    before/after measurement taken on the same instance, and why the
    contamination in *this* advance's before-measurement is still tolerable.
12. The gate is a choice and the saturation point is a derivation. Which
    parts of the arithmetic would change if the gate moved to 0.8, and which
    would not?
