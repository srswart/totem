---
advance:
  id: "ADV-CORE-005"
  title: "Quality evaluation: recall relevance + ranking behavior"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core", "store", "gateway"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 35
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: evaluation
  facets: [quality]
  work_products: []
  status: planned
---

## Objective

Evaluate recall quality once the value loop is wired (after ADV-CORE-002 +
ADV-STORE-002): does retrieval actually favor relevant, valuable, current
memory? Runs the recall-quality scorer (ADV-GATEWAY-008) over the golden
corpus (ADV-STORE-005) and dispositions findings.

## A known defect this evaluation must be able to detect (2026-08-08)

This advance was authored before any of it ran, and its Quality Risks section
already named the hazard: *"ranking regressions from the value/currency
multipliers"*. That risk has since been confirmed on the deployed instance,
which changes this advance from an open question into one with a known
answer it must reproduce.

**The defect (ADV-CORE-008).** Four queries against the deployment — two of
them verbatim copies of a stored record's own body — returned the same seven
records in the same order. `combined_score = relevance * value_score *
currency * category_weight`; `relevance_from_distance` is `1/(1+d)`, so
relevance varies by at most 3x across the entire cosine range, while
`value_score` is unbounded and is *raised by the act of being recalled*.
Whatever ranks highly becomes more valuable and ranks highly more often.

**Why this evaluation would have caught it, and why it currently would
not.** `crates/totem-gateway/tests/eval_quality.rs` asserts
`precision_at_1 == 1.0` today, and passes — on a system where recall
demonstrably ignores the query. The reason is that
`crates/totem-store/src/corpus.rs` never sets `value_score` or
`last_used_at`: the words do not appear in the file. Every golden record has
an identical, pristine economics block, so relevance is the only term that
varies and ranking looks perfect.

**The corpus measures the system in a state the system is never in.** A real
estate always has accumulated usage; a fixture never has, unless somebody
puts it there. That is the gap this advance has to close before its numbers
mean anything.

## Outcome

After this advance:
- Measured ranking quality against the golden set, per category, with the
  value × currency multipliers on and off — proving the value loop improves
  (not degrades) retrieval.
- Findings dispositioned: fixes filed as advances, accepted limitations
  recorded as residual risk.

## Planned Work

- [ ] branch: a fresh branch — nothing of this advance has been implemented
      (`pr_links: []`, `implementation_completed_at: ~`); the 2026-08-05
      `started_at` is when it was authored, not worked
- [ ] **corpus economics first.** Extend the golden corpus with records whose
      `value_score` and `last_used_at` differ: a well-used incumbent, a fresh
      exact match, and the tie cases between them. Without this the scorer
      cannot see the defect above, and its 1.0 is measuring an empty room.
- [ ] **reproduce ADV-CORE-008 through the scorer.** Success for this step is
      `precision_at_1 < 1.0` — a *failing* score on the current code. An
      evaluation that cannot fail is not an evaluation.
- [ ] run scorer over golden corpus: baseline (relevance only) vs. full ranking
- [ ] probe category weighting (Instructions priority, Context TTL, decayed Knowledge sinking)
- [ ] disposition findings; file follow-up advances for defects

## Bug Fixes

- [ ] None yet

## Quality Risks

- Ranking regressions from the value/currency multipliers; category weights
  inverting intent (e.g. stale Instructions outranking fresh Context);
  scope-chain merge dropping or duplicating relevant items.
- **Confirmed 2026-08-08**, the first of those: an `instructions` record with
  accumulated usage outranks an exact body match on another record. Reproduced
  in `crates/totem-store/tests/recall_ranking.rs` (`#[ignore]`d so the suite
  stays green while the defect stays executable).
- The risk this advance now runs itself: **a green evaluation that means
  nothing.** The scorer already reports a perfect 1.0 against a broken
  system. Any number produced before the corpus carries realistic economics
  should be treated as unmeasured, not as passing.

## Test Scope and Method

- Golden-query scoring (ADV-GATEWAY-008 scorer) on the seeded corpus
  (ADV-STORE-005); A/B of ranking formula terms; per-category breakdown.
  Out of scope: live-session reuse-rate measurement (needs production data).

## Coverage

- All six categories, all four scopes, aged and contested fixtures; both MCP
  and REST recall paths.

## Findings and Disposition

- To be completed at evaluation time; every finding gets a disposition
  (fixed / accepted / deferred with trigger).
- **Pre-registered finding**, so it cannot be quietly absorbed: the value
  loop currently overwhelms relevance without bound. Disposition: *fixed by
  ADV-CORE-008*. This evaluation's job is to measure it before and after,
  and to say whether the fix went too far — a system that ranks purely by
  relevance has thrown away the value loop, which is much of Totem's thesis
  (`docs/solution-intent.md` §4). Both directions get a number.

## Residual Risk

- Golden-corpus results are a proxy; the real metric is the brief's
  reuse-rate success measure, which needs production sessions to compute.
- Even with economics in the fixtures, a synthetic estate accumulates usage
  the way the author imagined rather than the way agents actually recall.
  The deployed instance found this defect in four queries; the corpus had
  not found it in weeks. Treat the corpus as a regression net, not as
  evidence that recall is good.

## Risk + Rollback

- Risk: evaluation is read-only against a seeded instance.
- Rollback: n/a — findings only.

## Evidence

- [ ] profile:selected-practices
- [ ] profile:honest-practices
- [ ] analysis:artifact

## CI Evidence Notes

- Attach scorer reports as the analysis artifact.

## Changes Made

- None yet
