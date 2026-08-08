---
advance:
  id: "ADV-CORE-008"
  title: "Recall ranks by what was asked: bound the value loop's grip on relevance"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store"]
  started_at: ~
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["behaviour_change"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
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

- `relevance_from_distance(d) = 1 / (1 + d)`. Cosine distance runs `[0, 2]`,
  so relevance runs `[0.33, 1.0]` — **a 3x range across every possible
  query**, from an exact match to a semantic opposite.
- `value_score` has **no bound**. `MemoryRepository::reinforce_usage` raises
  it on every record a recall returns.

So the records a recall returns become more valuable, and therefore get
returned more often. It is a positive feedback loop with no ceiling, and
after enough accumulation there is no query — not even a record's own text —
that can outrank the incumbent. On the deployment, an `instructions` memory
(the highest `category_weight`, and the most-recalled) wins everything.

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
saturating value score.**

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

## Behavioral Change

After this advance, a query whose text exactly matches a stored record ranks
that record first, regardless of any other record's accumulated history — and
the paraphrase queries EMB-004 measured behave semantically on the deployed
instance.

What must **not** change: a well-used memory still outranks an unused one
when relevance is comparable. If this advance turns Totem into a plain vector
index it has overshot, and the tests should say so in both directions.

## Planned Implementation Tasks

- [ ] branch / claim
- [x] decide the shape with Shawn — relevance gate + saturating value score,
      reasoning and rejected alternatives recorded above
- [ ] choose and justify the two constants (gate threshold, saturation point)
      from the corpus, not by tuning until tests pass
- [ ] test: an exact body match outranks an unrelated high-`value_score`
      record (currently `#[ignore]`d in `recall_ranking.rs` — un-ignore it)
- [ ] test: the converse — comparable relevance, and the well-used record
      still wins. The value loop must survive this advance.
- [ ] **corpus economics** — `crates/totem-store/src/corpus.rs` never sets
      `value_score` or `last_used_at`, so every golden record has identical
      pristine economics and `eval_quality` scores a perfect 1.0 against the
      broken system. Add a well-used incumbent, a fresh exact match, and the
      ties between them. Without this the evaluation cannot fail, and a fix
      cannot be shown to have fixed anything.
- [ ] feat: the scoring change
- [ ] golden queries on the deployed instance — the evidence ADV-STORE-008
      could not meet

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
