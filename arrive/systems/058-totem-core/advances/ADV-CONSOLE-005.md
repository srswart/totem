---
advance:
  id: "ADV-CONSOLE-005"
  title: "An inventory, not a recall: see what is actually stored and why it ranked"
  system: "058-totem-core"
  primary_component: "console"
  components: ["console", "gateway"]
  started_at: ~
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
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

**Shawn, 2026-08-08: "I need a way as an admin of totem to see more clearly
the memories we have captured, because right now it's not easy for me to know
what we have actually even captured in our dogfood corpus."**

The Memories tab exists and does not answer that question, because it is not
an inventory — it is a recall. `fetch_memories`
(`crates/totem-console/src/api.rs:137`) posts to `/recall` with a null query
and a fixed limit, so what it shows is *ranked, truncated, and
scope-merged*: the top N by a scoring function, presented as if it were the
contents. Three things follow:

- **You cannot tell what is missing.** Anything below the limit, or merged
  away by scope precedence, is simply absent with no indication.
- **The numbers that decide ranking are invisible.** `use_count`,
  `last_used_at`, `value_score`, `currency` are not shown, and they are
  exactly what would have explained the dogfood estate's behaviour during
  ADV-CORE-008.
- **Looking at it changed it** — see ADV-GATEWAY-017. That is fixed there;
  this advance must not reintroduce it.

## Behavioral Change

An **Inventory** view, distinct from recall:

- **Complete and paginated**, not top-N. If a scope holds 400 records the
  operator can reach all 400 and can see that there are 400 — a count is
  itself information the console has never shown.
- **Economics visible per record**: `use_count`, `last_used_at`,
  `value_score`, `currency`, plus `embedding_model` and `status`. These drive
  ranking and are currently unobservable from any surface.
- **Filters** by category, scope, tag, and embedding model. "Which records
  are still in the old vector space?" should be answerable by looking.
- **A "why did this rank here?" drill-down** onto ADV-GATEWAY-016's score
  breakdown, including records the gate excluded — so the answer to *"why
  isn't the record I expected in these results?"* is one click rather than an
  advance.
- **Non-reinforcing throughout.** The inventory must never write.

## Scope and Boundaries

**In scope:** the inventory view, its gateway endpoint if a paginated
non-ranked listing does not already exist, the filters, and the score
drill-down.

**Out of scope:** editing or deleting memories from the console (governance
actions have their own path); the recall view itself, which stays as it is
and remains the honest picture of *what an agent would receive*; curation.

## Design Notes

Keep the recall view. The two answer different questions — "what would an
agent get?" and "what is in here?" — and collapsing them is what produced the
current confusion. They should be visibly distinct, not two modes of one
list.

The inventory needs a listing endpoint that ranks nothing. If `/recall` is
reused with a large limit this advance has failed: it would still be ranked,
still truncated, still merged by precedence, and still shaped by the scoring
function whose behaviour the operator is trying to inspect.

## Risk + Rollback

- Risk: a paginated listing over a large scope is a new performance shape for
  the store. Note what it costs; ADV-GATEWAY-007's performance evaluation is
  where it gets characterised properly.
- Risk: showing every record in a scope is a wider disclosure surface than a
  ranked recall. Scope isolation must be enforced identically — the same
  store-layer rule, not an application-layer filter
  (`docs/solution-intent.md`).
- Rollback: the view is additive.

## Evidence

- [ ] tests:integration — the inventory returns records a ranked recall would
      have truncated away, which is the whole point.
- [ ] tests:integration — scope isolation holds on the listing path, proven
      with the corpus leak-bait pairs and not merely with a happy-path read.
- [ ] tests:unit — browsing the inventory leaves economics untouched.
- [ ] manual — Shawn can answer "what is in the dogfood corpus?" from the
      console. This advance is the one whose success is a person's judgement,
      and it should be recorded as such rather than dressed up as a metric.

## Check for Understanding

1. The Memories tab shows memories. Give three distinct reasons it does not
   answer "what have we captured?".
2. Why keep the recall view at all once an inventory exists?
3. Reusing `/recall` with a very large limit would be the small change. Name
   the properties that would still be wrong.
4. The inventory exposes more than a ranked recall does. Where must scope
   isolation be enforced, and why is that layer not negotiable here?
5. Which four per-record numbers would have explained the dogfood estate's
   ranking during ADV-CORE-008, and why was none of them visible from any
   surface?
