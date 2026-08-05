# Totem Effectiveness Evaluation — Design Notes

**Status:** Deferred until Totem is usable end-to-end (needs ADV-GATEWAY-002;
ideally ADV-CORE-002 so the value loop is what gets tested) · **Date:** 2026-08-05
· **Origin:** discussion while building phase 1; captured for later.

The question: how do we assess, credibly, whether Totem is actually helping —
on token cost and on outcome quality? The chosen approach is a **paired
twin-project experiment** (same test project built with and without Totem),
but designed around three traps that would otherwise make the result
unconvincing.

## The core idea

Two arms build the same ARRIVE-governed test project from the same starting
commit, same implementation plan, same routine prompts, same models. The only
delta: one arm is enrolled in Totem (recall/save available and encouraged by
its process notes); the other runs the status-quo baseline. Run the arms **in
parallel, not sequentially**, so model drift and the reviewer's own learning
don't become confounds.

## Trap 1 — n=1 against high variance

Agent runs are noisy: the same advance, same model, same context produces
meaningfully different token counts and quality run to run. A single
build-off cannot separate Totem's effect from that noise, and costs almost as
much as a designed experiment.

**Design response:** the unit of analysis is the **advance, not the project**.
A ~20-advance test project yields ~20 paired comparisons; look for a
consistent per-advance delta, not one aggregate number that two outliers can
own.

## Trap 2 — greenfield underweights Totem's value proposition

Totem's promise is *cross-session* context: time-to-context, remembered
decisions, shared landscape. Early advances in a fresh project have almost no
memory to draw on, so both arms look similar at the start — and a premature
read concludes "no effect."

Also be honest about the control: the baseline arm is not "nothing." It is
CLAUDE.md + ARRIVE artifacts + tech-direction docs in-repo, which is already a
decent single-repo memory substitute (this repo's own loop proves it —
ADV-STORE-006 built cleanly on ADV-STORE-004's recorded findings). Totem's
edge should appear where files-in-repo don't reach: episodic history,
cross-actor/cross-repo context, "why did we decide X," value-ranked recall
versus re-reading everything.

**Design response: plant memory-stress events in the project script** so
recall-or-re-derive moments are discrete and scoreable rather than diffuse:

- a design decision made early (session ~3) that silently matters late
  (session ~12);
- a contradiction introduced mid-project that *should* surface as an
  Uncertainty record rather than being paved over;
- a "second agent joins cold" event partway through — the arriving agent's
  time-to-context is measured directly;
- a return-after-context-loss event (fresh session, no carried context, must
  resume mid-advance).

## Trap 3 — unblinded judging

Whoever scores quality will know which arm they're reading — the Totem arm
literally cites memories. **Design response:** strip run artifacts of
identifying traces; use a blinded judge panel (LLM judges scoring each PR
against its advance spec on pre-registered criteria) with human spot-checks;
**pre-register the metrics and thresholds** before any results exist so
nothing is chosen after the fact.

## Metrics (per advance, per arm)

| Class | Metric | Source |
|---|---|---|
| Cost | Tokens per advance | Already plumbed: `advance.model_usage` via `arrive usage import` + `arrive/model-pricing.yaml` |
| Cost | Wall-clock, turn count | Journey reports / session transcripts |
| Time-to-context | Tool calls & file reads before first productive edit | Session transcripts; this is the brief's own success measure and where Totem should win most visibly |
| Core promise | **Re-derivation count** — times the agent re-investigates something already decided and recorded | Planted events make this directly scoreable |
| Quality | Blinded spec-adherence score | Judge panel |
| Quality | Review findings count/severity; rework rate (post-merge fixes touching the same code) | PR history |
| Quality | Reviewability score | `arrive score`, already in frontmatter |
| Quality | Planted contradiction surfaced vs silently resolved | Planted events |

## Cost accounting rule

Charge Totem's overhead honestly: recall/save tokens, curator runs,
enrollment, sync. The number that matters is **net**:
(tokens saved by arriving pre-briefed) − (tokens spent maintaining the memory
estate). A Totem that helps quality but costs more than it saves is a
different finding than one that does both — the design must be able to tell
them apart.

## Known asymmetry (accepted, not a flaw)

The Totem arm accumulates memory as it goes. That is not contamination — it
*is* the treatment. Do not try to reset it between advances.

## Sequencing and ARRIVE integration

- **Prerequisites:** ADV-GATEWAY-002 (agents can attach at all); ideally
  ADV-CORE-002 (value loop active, so retrieval ranking is what's tested).
- **Pairs with ADV-CORE-004:** the value-attribution experiment needs labeled
  sessions — exactly what this harness produces. Consider running them
  together.
- **Register as two advances** when scoped, following the established
  enablement→evaluation pattern (see docs/arrive-decomposition-gaps.md,
  "Robustness" section):
  1. *Enablement* — the twin-project harness: test project + implementation
     plan, planted memory-stress events, metrics collection, judging rig
     (work products: `test_data`, `test_automation`).
  2. *Evaluation* (quality + performance facets) — the blinded paired
     comparison itself, with findings dispositioned.
- **Raw data largely exists already:** both arms' `docs/agent-journey/`
  reports (frontmatter: outcome, score, checks) are the per-run record; the
  harness mainly adds pairing, planted-event scoring, and blinding.

## Bottom line

A single with/without build-off is nearly as expensive as the designed
experiment but cannot distinguish "Totem didn't help" from "the project never
exercised memory." Do the paired design with planted recall events, per-advance
pairing, blinded judging, pre-registered metrics, and honest net-cost
accounting — or don't bother running it.
