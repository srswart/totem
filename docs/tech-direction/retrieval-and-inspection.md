# Retrieval and inspection are different reads

**Status:** decided 2026-08-08 (Shawn). Governs ADV-GATEWAY-016,
ADV-GATEWAY-017, ADV-STORE-009, ADV-INFRA-007, ADV-CONSOLE-005.

## The decision

Totem has **two** kinds of read, and they must not share a path.

| | Retrieval | Inspection |
|---|---|---|
| Who asks | an agent, mid-work | an operator, an evaluation, a human |
| Question | "what should I know right now?" | "what is in here, and why did it rank that way?" |
| Result | ranked, truncated, scope-merged | complete, paginated, explicit |
| Scores | hidden | every factor, itemised |
| Side effects | **reinforces** what it returns | **none, ever** |

Retrieval is the product. Inspection is how anyone — including us — finds out
whether the product works.

## What forced it

Until now Totem had exactly one read path, `POST /recall`, and it is ranked,
truncated, scope-merged **and mutating**. Every consumer was pushed through
it, including two that should never have been:

- **The console's memory browser** (`crates/totem-console/src/api.rs`) listed
  memories by calling `/recall`. Opening the Memories tab therefore ran
  `reinforce_usage` over everything it displayed: `use_count += 1`,
  `last_used_at = now`, `currency = 1.0`. *The tool for looking at the corpus
  was a writer to it,* and some part of the dogfood estate's economics is an
  artifact of us looking.
- **ADV-CORE-008's own golden-query measurement.** The before capture
  reinforced all seven records to `currency = 1.0`, so by the after capture
  that term cancelled everywhere and the currency fix — the defect the corpus
  fixtures had just uncovered — went untested against the deployment. The
  hazard was written into the advance *before* the measurement was taken and
  the measurement walked into it anyway.

The second one is the argument. A single mutating read path does not merely
inconvenience an evaluation; it makes some measurements **impossible to take
correctly**, and it does so silently, because the contaminated number looks
exactly like the clean one.

## Three consequences

**1. Reinforcement measures agent use, not human attention.** This is a
product decision, not a test affordance. A human scrolling a list is not
evidence that a memory earned its keep, and counting it as such feeds the
value loop noise it cannot distinguish from signal. Reinforcement is a
property of the **caller**, not of a per-request flag a caller might forget —
so it binds to the credential. The access log records whether a read
reinforced, because "this read did not count" is itself an auditable fact.

**2. Anything that changes ranking must be answerable.** ADV-STORE-008 had to
add an endpoint before anyone could name the running embedder; ADV-CORE-008
hit the same wall one layer up and could not close its own diagnosis. The
rule generalises: **if the system makes a decision, you must be able to ask it
what it decided and why.** For ranking that means every factor — distance,
relevance, value, currency, category weight, the combined score, and whether
the gate excluded the record — not just the resulting order.

**3. A calibration corpus is data, not code.** A corpus compiled into the
binary cannot be versioned independently of the code under test, cannot grow
without bloating the image, and cannot be changed without a rebuild — which
on this project is measured in tens of minutes. It becomes a versioned
artifact with a manifest, carrying its golden queries **in the same file** so
the questions and the records cannot drift apart, and carrying each record's
economics, because ADV-CORE-008 proved that a corpus with uniform economics
cannot fail.

## Deliberately not decided here

- **Whether the calibration estate is a separate namespace, a separate
  database, or a separate deployment.** ADV-INFRA-007 decides it. The
  constraint it must satisfy: Episodic rows are append-only at the schema
  level, so a corpus cannot be wiped in place, and "reset" has to mean
  building a fresh container for it.
- **Whether `totem_feedback` should feed reinforcement.** Still open from
  ADV-CORE-008, still worth doing, still a separate change with its own
  evidence.
- **What the right embedding behaviour actually is.** ADV-CORE-008 left a
  live question — whether the real embedder genuinely ranks an unrelated
  instruction close to an exact answer. This document says how to find out,
  not what the answer is.
