---
advance:
  id: "ADV-GATEWAY-016"
  title: "Ranking is answerable: expose every score a recall computed"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "store", "core"]
  started_at: "2026-08-08T09:30:00Z"
  implementation_completed_at: "2026-08-08T10:15:00Z"
  review_time_estimate_minutes: 20
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: ["tests:unit", "deployment:executed"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
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

- [x] tests:unit — the explanation matches the ordering it claims to explain.
- [x] tests:unit — a gate-excluded record appears in the explanation and not
      in the results. Needed a hand-negated embedding: prose does not reach
      orthogonality, which turned out to be the finding above in miniature.
- [x] deployment:executed — re-ran against version 12, non-mutating.
      **Settled: it is a ranking problem.** The nearest record loses in 4 of
      6 queries, always to the same `instructions` memory, on
      `category_weight` alone. See "What it found".

## What it found, on first use

Run against the deployment 2026-08-08 (version 12), non-mutating,
`RECALL=0 scripts/golden-queries.sh` — `evidence/golden-queries-explained.txt`.

**ADV-CORE-008's open question is answered, and the answer is the opposite of
what was expected.** That advance guessed the residue was an embedding or
corpus problem, on the grounds that the surviving structural advantage was
only 1.13x. It is a **ranking** problem, and 1.13x was plenty.

| query | nearest record | winner | inverted |
|---|---|---|---|
| gateway owns the store | DEP-001 | DEP-001 | — |
| Cursor remote-MCP | MCP write path | `arrive plan check` | **yes** |
| which process opens the engine | DEP-001 | `arrive plan check` | **yes** |
| what we still don't know | Overnight experiment | `arrive plan check` | **yes** |
| worth saving to memory | `arrive plan check` | `arrive plan check` | — |
| Docker build failure | MCP write path | `arrive plan check` | **yes** |

**In 4 of 6 queries the nearest record loses to a record further away**, and
in every case to the same one — the `instructions` memory whose
`category_weight` is 1.0.

Worked, for "which process is allowed to open the SurrealDB engine?":

```text
DEP-001              dist 0.374  rel 0.728  cat 0.885  ->  0.644
arrive plan check    dist 0.427  rel 0.701  cat 1.000  ->  0.701
```

**The embedder gets it right.** DEP-001 is genuinely closer to the question
than the unrelated instruction is. Category weight then overturns it.

### The design error, stated precisely

ADV-CORE-008 derived every non-relevance bound from relevance's **theoretical**
range: the gate sits at orthogonality, so relevance spans `1 + gate` = 2x
among competing records, and each of the three other factors was given a
geometric third of that.

**Relevance's *realised* range on real text is 1.21x.** Every distance in the
whole run falls between 0.306 and 0.583 — a narrow band nowhere near the
gate. So:

- the **gate never fires**. Nothing approaches 1.0; it is dead code against
  real queries, which is why ADV-CORE-008's gate produced no visible
  improvement on five of six queries.
- the non-relevance budget of 2x is **larger than the spread it was meant to
  be smaller than**. Category weight alone (1.13x) is nearly the whole of
  relevance's actual range (1.21x), so it decides orderings routinely.

The error was reasoning from what cosine distance *can* be to what it *is*.
Both embedders tested agree: unrelated prose lands around 0.3-0.8, not near
2.0. A bound derived from the arithmetic bottom of the metric will always be
too loose for the part of it real text occupies.

### Handed on, not fixed here

This advance's scope forbids changing a single score, and that boundary is
worth keeping — the measurement is only trustworthy because the thing it
measured was not adjusted to suit it. The fix belongs to a follow-on, which
must decide between at least:

1. **Derive the budget from the observed spread**, not the theoretical one.
   Requires the spread to be measured per-corpus, which makes it a
   calibration question (ADV-STORE-009, ADV-INFRA-007).
2. **Rescale relevance within the result set** — normalise distances against
   the candidates actually returned, so relevance uses its full range on
   every query rather than a twentieth of it.
3. **Move the gate to where the distances are.** A gate at orthogonality is
   inert; one set from the observed distribution would exclude something.

(2) is the most likely right answer and the most invasive, since it makes a
record's score depend on its competitors. None of them should be chosen
without the calibration corpus, because all three are tuning against a
7-record estate otherwise.

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
