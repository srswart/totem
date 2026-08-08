---
advance:
  id: "ADV-GATEWAY-017"
  title: "Reads that do not reinforce: reinforcement measures agent use, not attention"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "store", "console"]
  started_at: ~
  implementation_completed_at: ~
  review_time_estimate_minutes: 25
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

**Every read in Totem writes.** `recall()` calls `reinforce_usage` on
everything it returns — `use_count += 1`, `last_used_at = now`,
`currency = 1.0` — and there is no path that does not
(`crates/totem-store/src/memory.rs:582`). Two consequences are already
observed, not hypothesised:

1. **The console browses by calling `/recall`**
   (`crates/totem-console/src/api.rs:137`). Opening the Memories tab
   reinforces every record it displays. The tool for looking at the corpus is
   a writer to it, and an unknown fraction of the dogfood estate's economics
   is an artifact of us looking at it.
2. **ADV-CORE-008's measurement contaminated itself.** The before capture
   pinned `currency = 1.0` on all seven records, so by the after capture the
   currency term cancelled everywhere and the currency bound — the defect the
   corpus fixtures had just found — was never exercised against the
   deployment at all.

The second is the serious one. A single mutating read path does not merely
inconvenience an evaluation; it makes some measurements **impossible to take
correctly**, silently, because a contaminated number is indistinguishable
from a clean one.

## This is a product decision, not a test affordance

`docs/tech-direction/retrieval-and-inspection.md`: **reinforcement measures
agent use, not human attention.** A human scrolling a list is not evidence
that a memory earned its keep, and counting it as such feeds the value loop
(ADV-CORE-002) noise it cannot tell from signal. The console has been
generating exactly that noise since it shipped.

## Behavioral Change

- A read can be **non-reinforcing**, and this is a property of the **caller**,
  not a per-request flag every caller must remember to set. A credential
  carries whether its reads count. Forgetting is then impossible rather than
  merely discouraged.
- **The console's browse is non-reinforcing.** So is any evaluation or
  calibration read (ADV-INFRA-007).
- An agent's `recall` during real work still reinforces. The value loop is
  not being switched off; it is being pointed at the signal it was meant to
  measure.
- **The access log records whether a read reinforced.** "This read did not
  count" is an auditable fact, and without it a future investigation cannot
  reconstruct why an economics figure moved — or did not.

Whoever implements this must decide, and record, what happens to a caller
whose credential says non-reinforcing but which asks for reinforcement
anyway: refuse, or silently honour the credential. Refusing is the honest
default — a silent downgrade is how a calibration run quietly becomes a
production read — but it is a decision, not an obvious consequence.

## Scope and Boundaries

**In scope:** the credential property, the store-side non-reinforcing read
path, the access-log field, and switching the console to it.

**Out of scope:** back-correcting the economics already inflated by console
browsing — see "Risk" below; whether `totem_feedback` should feed
reinforcement (still open from ADV-CORE-008); score visibility
(ADV-GATEWAY-016).

## Risk + Rollback

- Risk (`behaviour_change`): records that were being kept current by console
  views will begin to decay. That is the correct behaviour and it will look
  like a regression — currency figures on the dogfood estate will drop after
  this ships. Say so in the runbook rather than letting it surprise someone.
- **The existing estate cannot be cleaned.** There is no record of which
  reinforcements came from browsing, so the inflation already in the dogfood
  corpus is unrecoverable. This is an argument for calibrating against a
  purpose-built corpus (ADV-STORE-009) rather than trying to rehabilitate
  this one, and it should be stated plainly rather than quietly worked
  around.
- Rollback: revert; reads reinforce again.

## Evidence

- [ ] tests:unit — a non-reinforcing read leaves `use_count`,
      `last_used_at` and `currency` untouched, asserted on the stored row and
      not merely on the response.
- [ ] tests:unit — a reinforcing read still reinforces. This advance must not
      turn the value loop off by accident.
- [ ] tests:integration — the access log distinguishes the two.

## Check for Understanding

1. Reinforcement binds to the credential rather than the request. Give the
   failure mode that choice prevents, and the cost it imposes.
2. The console has been reinforcing every memory it displayed. Why can that
   not be undone, and what does that imply about which corpus we calibrate
   against?
3. After this ships, currency figures on the dogfood estate will fall. Why is
   that the correct outcome, and why is it worth writing into the runbook
   before it happens?
4. ADV-CORE-008 documented this exact hazard *before* taking a measurement
   and was contaminated by it anyway. What does that suggest about relying on
   a written protocol where a mechanism is available instead?
5. A caller with a non-reinforcing credential asks for reinforcement. Argue
   for refusing, then for silently honouring the credential. Which is safer
   for a calibration run, and why?
