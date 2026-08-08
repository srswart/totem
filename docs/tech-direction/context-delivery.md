# Tech Direction: Context delivery — what Totem owns, what it facilitates

**Status:** Decided (CTX-001..CTX-005) · **Date:** 2026-08-08 · **Advances:**
none yet (governs the ranking fix, work-context retrieval, and any source
connector) · **Companion:**
[organizational-memory.md](organizational-memory.md) (ORG-001..004, the ingest
posture this builds on), [retrieval-and-inspection.md](retrieval-and-inspection.md),
[value-attribution.md](value-attribution.md)

Teams today embed working context in their repositories — `CLAUDE.md`,
`.cursorrules`, a `docs/` folder. That has three costs, and the third is the
one that matters:

- **It loads unconditionally.** Every session pays for every rule, whether it
  is touching auth or CSS.
- **It duplicates.** "How we review code" exists in twelve repos and drifts in
  twelve directions. The copy *is* the distribution mechanism, so there is no
  such thing as one rule with many consumers.
- **Staleness is invisible.** A line written eight months ago is
  indistinguishable from yesterday's. The problem is not that repo context goes
  stale — everything does — but that **nothing about it says so**, and nobody
  deletes a line they cannot prove is dead.

Totem's opportunity is not to hold that content better. It is to **deliver it
conditionally, with provenance, and to know when it has gone off**. Decided by
Shawn on 2026-08-08 from the discussion recorded here.

## CTX-001 — Totem is a retrieval layer; for external content it facilitates rather than owns

**Decision:** Totem holds two kinds of content and its role differs by kind.

| | **Emergent** | **Projected** |
|---|---|---|
| Examples | a dead end someone hit; a decision and its reasoning | design-system usage, golden paths, component catalogues |
| Source of truth | Totem | somewhere else |
| Who writes it | agents, mid-work | a team that already maintains it |
| Totem holds | the record | a projection, plus a link back |

What unifies them is not the content. It is the **delivery**: the right slice,
at the right moment, with provenance.

**Why this rather than a knowledge platform.** Owning content means building
authoring, review, approval, permissions on documents, version history,
migration, and a UI good enough that people actually write there. That is
Confluence, and it is out of reach. Facilitating means the source keeps all of
it — Totem builds ingestion, structure-preserving projection, freshness,
retrieval and feedback. Roughly a third of the surface, and none of the
expensive human-facing parts.

**The consequence worth sitting with: ranking is the product, not a feature.**
If Totem's value is "the right thing at the right moment", retrieval quality
*is* the thing, and the store, the sync and the console are plumbing around
it. This is why the ranking defect (ADV-GATEWAY-016) is not a bug delaying the
roadmap — it is the core capability not yet working.

**Cost scales with source *types*, not content volume.**
`ADV-ARRIVE-SYNC-001` is one connector and a real chunk of the codebase; the
tenth will cost about what the first did. The strategy is therefore few source
types gone deep, never broad ingestion.

## CTX-002 — Segmentation is a relevance concern; security and relevance use different mechanisms

**Decision:** Keeping the iOS team from seeing the web design system is a
**noise** problem, not a **security** problem, and it must not reuse the
security mechanism.

The failure modes invert, which is what forces different layers:

| | Security isolation | Relevance segmentation |
|---|---|---|
| Worst outcome | leak — irreversible | noise — recoverable |
| Must fail | **closed** | **open** |
| Wrong exclusion | inconvenient | **worse than wrong inclusion** |
| Enforced at | store layer, exactly | ranking layer, softly |

The third row decides it. **A wrong exclusion is invisible and unauditable** —
an iOS engineer who one day builds a web view gets nothing, and has no way to
discover that something existed. You cannot audit what you never received.
Same argument as "absent is worse than stale".

So, two mechanisms, deliberately not merged:

- **The scope chain** (`actor` / `project` / `team` / `platform`) stays what it
  is: hard isolation, enforced in the store, fail-closed. Correct for
  actor-private memory and the occasional genuinely sensitive record. It should
  stay **rare** — every use is a wall somebody eventually needs to cross.
- **Relevance segmentation** is soft, applied in ranking, fail-open. Everything
  about which design system a team cares about lives here.

Using the scope chain for relevance is the tempting shortcut and a bad one: it
inherits hard-exclusion semantics, makes every new team an administration task,
and breaks precisely on the cross-cutting work — shared components, migrations
— where good context matters most.

**Residual, stated as a rule rather than a mechanism:** sensitive content is
mostly not ingested at all. That is a good default and a poor guarantee.
Whether anything *enforces* it, or it remains a convention like
`GENERATOR_TAG`, is open (see below).

## CTX-003 — Retrieval is keyed on the work, not only on the query text

**Decision:** The primary retrieval key is the **work context** — repo,
advance, components, files in play — not a free-text query. Semantic recall
remains, for the open questions it is good at.

**Why:** vector similarity on a query requires the agent to already know what
it does not know. To retrieve the forms golden path by asking "how do I build a
form", you must already suspect one exists. The agent that most needs it is
exactly the one that will not ask. ARRIVE already knows the work context, and
Totem already has `subject: {kind, id}` and the landscape graph, so the
structural key exists and is unused.

This should be **pushed when the work context is established**, not pulled on
demand. Relying on discipline has already failed once: ADV-CORE-008 documented
a measurement hazard and then walked into it in the same advance.

Two binding conditions:

- **There must be an always-on floor.** "Never push to master" cannot depend on
  a retrieval succeeding. Some rules are safety, not context, and load
  unconditionally. Note that `injection_priority` was the field trying to
  express this tiering, and it is the same number that caused the ranking
  defect by also acting as a ranking weight — the concept is real, the
  conflation is not.
- **Most segmentation then falls out for free.** Every motivating example
  segments on *what the work is about*, not who is asking. If retrieval is
  keyed on "repo R, components X and Y", a web design-system entry simply does
  not match an iOS component — no configuration required. What does *not* fall
  out is the unbound question ("how do we handle empty states?"), which needs a
  declared applicability on the record, matched softly and contributing to
  rank rather than gating.

## CTX-004 — Projected content is invalidated by its source, not by decay

**Refines ORG-002**, which established that ingested documents carry a lifecycle
and participate in currency/decay. That holds for documents Totem is the
custodian of. For content with a live external source it is a weaker instrument
than we now have available.

**Decision:** where a source of truth exists, **source-change invalidation
replaces decay**.

- Decay-by-non-use is a *proxy* for staleness. A design-system page does not
  become less true because nobody read it this month, and ranking authored
  guidance by usage means the most-read component wins — which is how everyone
  ends up using `Button` and nobody discovers `FormField`.
- Source-change is a *direct* signal, and strictly better where obtainable.
- The value loop (`use_count`, citation boost) measures whether an emergent
  memory earned its keep. A golden path's authority comes from its owner. The
  loop should not apply to projected content at all.

**The harm is asymmetric, which is why this matters.** An emergent memory that
is wrong costs somebody an hour. **An authored path that is wrong is a defect
factory** — agents follow it precisely. So Totem must *know* when a projection
is stale and say so; silently serving a stale projection is the exact harm this
direction exists to remove.

## CTX-005 — Layered guidance composes; it does not override

**Decision:** scope precedence — narrower wins entirely — is correct for one
fact held at two scopes, and wrong for layered guidance.

```
platform: all forms use FormField
project:  …except the legacy admin, which uses RawInput
```

Today the reader gets only the project record and never learns the general
rule. Composition semantics — both, with the relationship visible — are
required before any design-system content is ingested, or the first ingestion
silently loses the platform layer.

This is a genuine modelling gap in `merge_chain`, not a configuration choice.

## What must be true before authored content lands

Three gates, in order. Each is a way this direction fails quietly:

1. **Retrieval is trustworthy.** A stale `CLAUDE.md` line at least *arrives*. A
   memory ranked 8th when the limit is 7 does not exist as far as the agent is
   concerned — we would have traded *stale but present* for *absent and
   invisible*. ADV-GATEWAY-016 measured the nearest record losing 4 times in 6.
   Until that is fixed and measured (ADV-CORE-005), nobody should be asked to
   delete their `CLAUDE.md`.
2. **Authored guidance has a review path.** A `CLAUDE.md` change goes through a
   PR and somebody approves it. A memory saved mid-session is reviewed by
   nobody. Without this we trade *reviewed but stale* for *fresh but
   unreviewed*. `governance.review` exists and nothing exercises it.
3. **Degraded operation is defined.** A repo file works offline and cannot be
   down. If Totem is unavailable, does work stop or degrade? A cached local
   projection would answer it. This is the strongest practical argument for the
   status quo and it deserves a real answer, not a shrug.

## The falsifiable test

Written down now, while it is cheap and before anyone is invested in the
answer:

> **Can we delete something from a repo and have nobody notice the absence —
> because the guidance arrived anyway?**

A real user outcome rather than a metric, and it can be tried on this repo's own
`CLAUDE.local.md` first.

## Open questions

- **Enforcement of "do not ingest sensitive content"** — convention, or
  mechanism? (CTX-002)
- **What a connector actually looks like** for a structure-bearing source. A
  component catalogue flattened for vector search loses "Button composes Icon";
  how much structure must survive, and is that per-source-type work? (CTX-001)
- **How applicability is declared and matched** for unbound queries (CTX-003).
- **Whether composition is a merge-time or read-time concern** (CTX-005).

## Consequences

- The ranking fix and ADV-CORE-005 are **prerequisites for the product
  direction**, not maintenance. This is the strongest argument yet for
  fundamentals-first sequencing.
- The calibration corpus (ADV-STORE-009) should grow **cross-segment
  near-misses** — two design systems saying similar things about different
  platforms. That is much closer to the real discrimination problem than
  anything currently in `calibration-v1`, and it is the same evaluation
  machinery, so noise becomes measurable as precision.
- Work-context retrieval (CTX-003) and relevance segmentation (CTX-002) are
  **one piece of work, not two**.
- Leader-facing views are aggregation over the **access log**, not restrictions
  on content — a different and easier mechanism than either above.
- The first connector should target the source type with the worst duplication
  pain, and should be exactly one connector rather than a framework for
  connectors.

## Revisit triggers

- Retrieval precision stays poor after the ranking fix — CTX-001's "ranking is
  the product" would make that an existential finding rather than a defect.
- A source type appears whose content genuinely is sensitive, reopening
  CTX-002's residual.
- The always-on floor (CTX-003) grows past a handful of rules, which would mean
  the conditional-delivery premise is not holding.
