---
advance:
  id: "ADV-GATEWAY-016"
  title: "Ranking is answerable: expose every score a recall computed"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "store", "core"]
  started_at: ~
  implementation_completed_at: ~
  review_time_estimate_minutes: 20
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

**ADV-CORE-008 could not finish its own diagnosis, and this is why.** Recall
returns records and nothing else — no distance, no per-factor score, no
indication that the gate excluded anything. So when five of six golden
queries kept returning the same unrelated `instructions` record first, there
was no way to ask the deployment *why*. The advance closed with the cause
recorded as a hypothesis (the surviving structural advantage is 1.13x, so the
weights are probably not it) that nobody could confirm or refute.

This is the same defect ADV-STORE-008 hit one layer down: a deployment
serving non-semantic recall looked exactly like one doing the real thing, and
stayed that way until `/admin/embedding` made the running model nameable.
Ranking is now the least observable part of the system.

`docs/tech-direction/retrieval-and-inspection.md` is the governing decision:
**if the system makes a decision, you must be able to ask it what it decided
and why.**

## Behavioral Change

A caller can ask for a recall's reasoning and receive, per candidate:

- the raw vector `distance` (or its absence, when the query carried no probe)
- `relevance`, `value`, `currency`, `category_weight` — each as actually
  applied, i.e. after the ADV-CORE-008 transforms, not the raw stored values
- the `combined` score
- whether the record was **excluded by the gate**, which is invisible today:
  a gated record simply does not appear, and a caller cannot distinguish
  "nothing matched" from "everything was filtered"

Records excluded by the gate must appear in the explanation *with their
scores*, precisely because they are absent from the result. The most useful
question this endpoint answers is "why is the thing I expected missing?", and
an explanation that omits the missing records cannot answer it.

**The explanation never reinforces.** Asking why must not change the answer.
See ADV-GATEWAY-017 for the general rule; this advance may not wait for it,
so it establishes the non-mutating path for its own route regardless.

## Scope and Boundaries

**In scope:** the score breakdown, its route and DTO, the gate-exclusion
reporting, and the store-side plumbing to carry per-factor scores out of
`rank_score` instead of discarding them.

**Out of scope:** the console surfacing it (ADV-CONSOLE-005); changing any
ranking behaviour — **this advance must not alter a single score**, and a
test should pin that the ordering with and without explanation is identical;
the calibration corpus (ADV-STORE-009).

## Design Notes

`rank_score` currently returns `f32` and throws the components away. The
minimal honest change is to have it produce a small struct that keeps them,
with the scalar derived from it — so the explanation is *the same arithmetic
the ranking used*, not a reimplementation that can drift. A second code path
that recomputes scores for display would be a defect generator: it would
agree right up until it mattered.

Whether this is a separate route (`POST /recall/explain`) or a flag on
`/recall` is the implementer's call, recorded with reasoning. A flag risks a
caller enabling it in production and paying for work it ignores; a separate
route duplicates the request DTO. Note that `explain_recall` already exists
for the *query plan* (TD-002/TD-003) — this is its sibling for *scores*, and
the naming should not collide confusingly.

## Risk + Rollback

- Risk: the explanation drifts from the ranking. Mitigated by deriving both
  from one computation; a test should assert the explained `combined` equals
  the score that produced the order.
- Rollback: the route is additive; removing it restores today's behaviour.

## Evidence

- [ ] tests:unit — the explanation matches the ordering it claims to explain.
- [ ] tests:unit — a gate-excluded record appears in the explanation and not
      in the results.
- [ ] deployment:executed — **re-run `scripts/golden-queries.sh` against the
      deployment with explanations**, and settle ADV-CORE-008's open
      question: is the unrelated `instructions` record winning on relevance
      (an embedding or corpus problem) or on something else (a ranking
      problem that survived CORE-008)? Record the distances.

## Check for Understanding

1. ADV-CORE-008 measured a surviving structural advantage of 1.13x against
   relevance's 2x. Why does that number make the *weights* an unlikely cause,
   and why is it still not evidence that the embedder is the cause?
2. Gate-excluded records are absent from results. Why must they nonetheless
   appear in the explanation, and which question becomes unanswerable if they
   do not?
3. Why must the explanation be derived from the same computation as the
   ranking rather than recomputed for display? Name the failure mode.
4. This advance forbids changing any score. What would go wrong with the
   evidence in ADV-CORE-008's record if it did?
5. `explain_recall` already exists and explains something else. What, and why
   is that distinction worth keeping clear in the naming?
