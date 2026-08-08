---
advance:
  id: "ADV-CORE-009"
  title: "Relevance decides: derive ranking from the distances real text produces"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store"]
  started_at: "2026-08-08T18:00:00Z"
  implementation_completed_at: "2026-08-08T18:40:00Z"
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 11
  risk_flags: ["behaviour_change"]
  evidence: ["tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
---

## Objective

ADV-GATEWAY-016 measured the defect precisely: **in 4 of 6 deployed queries
the nearest record lost**, always to the same `instructions` memory, on
`category_weight` alone.

```text
which process is allowed to open the SurrealDB engine?
  DEP-001            dist 0.374  rel 0.728  cat 0.885  ->  0.644
  arrive plan check  dist 0.427  rel 0.701  cat 1.000  ->  0.701
```

The embedder was right. DEP-001 genuinely is closer. Ranking overturned it.

**The root cause is one line.** `relevance_from_distance(d) = 1 / (1 + d)` is
nearly flat across the band real text occupies. Measured: BGE-small-en-v1.5
puts every distance in `0.31..0.58`, so relevance varied by **1.21x** across
an entire query — while category weight alone contributed **1.13x**. A factor
meant to be decisive was worth barely more than a factor meant to break ties.

ADV-CORE-008 had bounded the non-relevance factors against relevance's
**theoretical** range — the span from an exact match to the gate at
orthogonality, 2x. That derivation is sound arithmetic about a range real text
never enters. The band it actually occupies is a fifth as wide.

## The fix

Two constants, one derived from the other, and both stated against measurement
rather than chosen to make tests pass.

**`NON_RELEVANCE_BUDGET = 2.0`** — the only free choice. *History may matter as
much as a materially better match, and never more.*

**`MATERIAL_DISTANCE_GAP = 0.10`** — what "materially better" means, in the
units the store actually produces. About a third of the real embedder's whole
working band (0.277 wide), so comfortably beyond noise.

**`relevance_sharpness = ln(budget) / gap = 6.93`** — derived, and the
transform becomes `exp(-k·d)`.

Why an exponential: it has a **constant ratio per unit of distance**. "A fixed
distance gap is worth a fixed multiple" is not expressible under `1/(1+d)`,
whose value depends on where in the range you stand. It also means the claim
holds whatever absolute band an embedder uses — which is what stops this being
tuned to one model.

On the pair that motivated the advance, whose gap is 0.053 — half a material
gap — relevance is now worth 1.44x against category's 1.13x. DEP-001 wins.

**The gate stays, and is now honestly described as a backstop.** It sits at
orthogonality and ADV-GATEWAY-016 measured that it never fires: unrelated prose
reaches 0.767 (deterministic) and 0.824 (deployment), never 1.0. Suppressing
distant records is now the transform's job, smoothly, rather than a threshold's
job that nothing crosses. The gate remains for genuinely anti-correlated
vectors, where "does not compete" should mean absent.

## Measured

`calibration-v1`, seeded in-process: **7/9 before, 9/9 after.**

The headline query — `relevance_outranks_category_within_a_topic`, an
`Instructions` record beating a more relevant `Knowledge` one — now passes.
That is ADV-GATEWAY-016's defect, closed.

### Two of those nine were my own fixtures, and the corpus caught them

Both remaining failures after the scoring change were **defects in
ADV-STORE-009's corpus, not in the ranker**. Recorded because the temptation
to adjust the ranker until a fixture passes is exactly how the original bug
was reasoned into existence.

**`a_well_used_memory_still_wins_at_comparable_relevance`** claimed comparable
relevance and delivered distances of **0.233 and 0.544** — a gap of 0.311,
*three material gaps*. It was quietly a relevance test, and it had been passing
only because relevance was too weak to win. Repaired so the two bodies differ
by one word the probe does not contain: the gap is now **0.0103**, and the
well-used record wins on `value` (1.14x) against relevance's 1.07x.

*Honest caveat:* relevance still marginally favours the winner, so this is not
a pure economics test — value is merely the larger term. Making the *unused*
record the closer one would settle it outright and is worth doing when the
corpus next grows.

**`an_unrelated_topic_does_not_intrude`** could never have passed. Recall
returns its limit's worth of rows whether or not they are any good, so a record
scoring 0.012 against a winner's 0.730 still counted as "present". `CorpusQuery`
gains a **`limit`** (default 5): the question the corpus asks is about a
context window, and a context window has a size. `expect_absent` is meaningless
without one.

## Behavioral Change

Every recall's ordering changes. Relevance now dominates unless two records are
within a fraction of a material gap, where history decides — which is the
intended shape and the thing ADV-CORE-008's converse tests exist to protect.

## Risk + Rollback

- Risk (`behaviour_change`): all ranking moves. Computed at read time, so a
  revert is complete and immediate.
- Risk: over-correction into a plain vector index. Guarded at unit level
  (`a_well_used_memory_still_beats_an_unused_one_at_comparable_relevance`,
  `a_fresher_memory_...`) and end-to-end by the repaired corpus query. Both
  pass.
- **Unverified:** this is measured against the deterministic embedder and
  against distances *recorded* from the deployment, not against a fresh
  deployed run. `MATERIAL_DISTANCE_GAP` is calibrated to BGE-small-en-v1.5's
  observed band; a different embedder would want it re-derived, and nothing
  currently checks that. See ADV-CORE-005.

## Evidence

- [x] tests:unit — the constant-ratio invariant holds at every distance, not
      just at the endpoints. Stronger than the property it replaces.
- [x] tests:integration — `calibration-v1` 7/9 -> 9/9.
- [ ] deployment:executed — deferred to the next deploy, which also carries
      ADV-INFRA-008's unverified build-cache claim.

## Check for Understanding

1. `1/(1+d)` and `exp(-k·d)` are both monotonically decreasing in distance.
   Name the property the second has that the first lacks, and why it is the
   one that matters here.
2. ADV-CORE-008 derived its budget from relevance's theoretical range and the
   arithmetic was correct. What made the conclusion wrong anyway?
3. The gate is kept but described as a backstop. What does it still catch, and
   what did we wrongly expect it to catch?
4. Two of the nine corpus queries failed after the fix and neither was a
   ranker defect. Say how you would tell those two cases apart, and why
   getting it wrong is worse here than elsewhere.
5. `expect_absent` could not pass before a `limit` existed. Explain, and say
   what that implies about assertions of absence in a ranked system.
6. `MATERIAL_DISTANCE_GAP` is calibrated to one embedding model. What breaks
   if the model changes, and what would detect it?
