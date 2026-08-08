---
advance:
  id: "ADV-GATEWAY-017"
  title: "Reads that do not reinforce: reinforcement measures agent use, not attention"
  system: "058-totem-core"
  primary_component: "gateway"
  components: ["gateway", "store", "console"]
  started_at: "2026-08-08T10:45:00Z"
  implementation_completed_at: "2026-08-08T11:10:00Z"
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 12
  risk_flags: ["behaviour_change"]
  evidence: ["tests:unit"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
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

## What was built, and two things deliberately not

**The console no longer reinforces.** It now browses through
`POST /recall/explain` — the non-mutating route ADV-GATEWAY-016 built — rather
than `POST /recall`. Same records, same order, no write. This is the whole of
the observed bug, and it is fixed.

**The store gained `recall_observing`**, the same read metering nothing, with
`Reinforcement::Count | Skip` naming the choice at every call site. Tests
assert both directions on the *stored rows*: observing meters nothing, and an
ordinary recall still does, so the value loop cannot have been switched off by
accident.

### Not built: the credential flag

The advance specified reinforcement as a property of the **credential**. It is
not built, and the reason is that ADV-GATEWAY-016 made it unnecessary for
every consumer that exists:

- **The console** is served by the explain route, which cannot reinforce by
  construction — a stronger guarantee than a credential flag, because there is
  no code path to get wrong.
- **The evaluation harness** is served the same way:
  `RECALL=0 scripts/golden-queries.sh` reads only from `/recall/explain`.
- **ADV-INFRA-007's calibration estate** will measure through the same route.

That leaves the flag with no consumer. Building it would mean a schema
migration, a registry field, a CLI option and an OAuth discriminator, all in
service of a need nobody has yet — and the OAuth half cannot even be built
honestly today: WorkOS tokens are parsed for `sub` and `iss` only, and the
console and the cloud routines authenticate through the *same* AuthKit issuer,
so "OAuth means observer" would wrongly silence the routines' reinforcement.

**If a consumer appears** — a programmatic reader that needs plain records
without scores and must not meter them — `recall_observing` is already there
and only the plumbing is missing. Recorded so the next person knows it was
declined rather than overlooked.

### Not built: an access-log boolean

The advance asked for the log to record whether a read reinforced. It already
does, in the field that was there: `endpoint` distinguishes `/recall` from
`/recall/explain`, and those *are* the reinforcing and non-reinforcing reads.
A separate boolean would be a second, migratable copy of a fact already
present, and the two could disagree.

This changes if the credential flag is ever built — then a `/recall` call may
or may not meter, `endpoint` stops carrying the answer, and the boolean earns
its migration.

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

- [x] tests:unit — a non-reinforcing read leaves `use_count`,
      `last_used_at` and `currency` untouched, asserted on the stored row and
      not merely on the response.
- [x] tests:unit — a reinforcing read still reinforces. This advance must not
      turn the value loop off by accident.
- [x] tests:integration — the access log distinguishes the two, via
      `endpoint`, which already carried the distinction. See "Not built: an
      access-log boolean".

## Check for Understanding

1. Reinforcement was specified as a property of the credential and was built
   as a property of the *route* instead. Say why the route is the stronger
   guarantee, and name the case it does not cover.
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
