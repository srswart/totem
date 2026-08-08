# Tech Direction: Organizational memory — loading and sharing posture

**Status:** Decided (ORG-001..ORG-004) · **Date:** 2026-08-06 · **Advances:**
none yet (governs future arrive-sync, codemap, and enrollment work) ·
**Companion:** [project-brief.md](../project-brief.md),
[solution-intent.md](../solution-intent.md) §2.2 (scopes), §6 (Codemap),
[deployment.md](deployment.md)

Totem's target role is **organizational technical memory**: a provenance-bearing
record of what teams build and have built, at the scale of ~10 teams and ~75
projects. That framing forces three questions the Solution Intent left open:
what to load from source code, what to load from documents, and whether
org-scale sharing is asking for trouble. Decided by Shawn on 2026-08-06 from
the discussion recorded here.

## The questions organizational memory must answer

The loading decisions below are derived from what cross-team consumers
actually ask. None of these require source bodies; all require structure,
contracts, decisions, and history:

- **Archaeology** — why does this system work this way? (decision → advance →
  PR → episodic record)
- **Prior art** — has anyone here already built this? (fastest-payback query)
- **Dependency confidence** — can I build on this; who owns it; what stage;
  what invariants; what's churning?
- **Intent-vs-implementation drift** — where did what we built diverge from
  what we said we'd build? (possible only because Totem holds both the intent
  docs and the advance records)
- **Agent priming** — every agent starts knowing team conventions and platform
  decisions, giving convention consistency across projects without copies.
- **Incident/audit response** — what changed recently across everything that
  touches X?

## ORG-001 — Source code is referenced, never ingested

**Decision:** Totem stores **pointers, contracts, and summaries — not source
bodies**. Git remains the sole authoritative store of source. The
representation is layered:

1. **Structure** — systems, components, roots, stages (already ingested via
   `arrive-sync`).
2. **Public interfaces and contracts** — API surfaces, schemas, events, data
   contracts, stored **verbatim**. This is the one place actual code text
   belongs: the interface *is* the shareable artifact, and it is the natural
   cross-team boundary (interfaces promote outward; internals stay
   project-scoped).
3. **Curated semantic summaries** — per-component "what / why / key
   invariants", AI-drafted, human-reviewed, each carrying a provenance pointer
   `repo + path + commit SHA`. The SHA gives exact provenance without
   duplication; consumers who need the body follow the pointer to git.
4. **Change profile** — churn, hotspots, advance linkage (the deferred Codemap
   of Solution Intent §6). Metadata only; stays deferred until 1–3 prove out.

**Why:**

- A second copy of source is a staleness liability and a retrieval-noise
  generator; git already versions it perfectly.
- Security blast radius: contracts and summaries leaking is bad; 75 projects'
  source concentrated in one database leaking is catastrophic (see
  [deployment.md](deployment.md) — one process, one data directory).
- Cross-team consumers ask "what does this do / can I depend on it / what
  changed" — never "show me all the code."

## ORG-002 — Curated documents are first-class ingest, with lifecycle

**Decision:** Project briefs, solution intents, and tech-direction documents
**are loaded** — they are the highest-value ingest Totem can perform:
low-volume, human-curated, and holding exactly the "why" that evaporates when
people leave. Two binding conditions:

- **Lifecycle, not archive.** Every ingested doc carries status
  (draft / active / superseded) and participates in currency/decay. A stale
  intent presented as current truth is worse than no memory. **Refined by
  CTX-004** ([context-delivery.md](context-delivery.md)): where a document has
  a live external source of truth, source-change invalidation replaces decay —
  decay is a proxy for staleness and source-change is a direct signal. When a doc
  contradicts what was actually built, that is an **Uncertainty** record, not
  a silent preference for either side.
- **Linked to advances.** Docs are connected to the advances that implement
  them; that linkage is what makes intent-vs-implementation drift queryable.
  An unlinked doc is a PDF graveyard with embeddings.

## ORG-003 — Enrollment is tiered; full memory trails curation capacity

**Decision:** Enrollment is not binary. Three tiers, rolled out in order:

| Tier | Loads | Gate |
|---|---|---|
| 1 — Landscape | ARRIVE artifacts + briefs/intents/tech-direction docs | None — safe for all ~75 projects quickly |
| 2 — Contracts | Public interfaces, schemas, curated component summaries | Light owner review, per team |
| 3 — Full memory | Episodic/working memory from live agent sessions | Value metrics stay healthy (reuse rate, staleness); curation capacity in place |

**Why:** 75 repos is trivial for the database; the real risk is memory
*quality* at scale. One team's unfiltered agent-written memory is manageable;
75 projects' worth is a swamp within months. At org scale the value/currency
loop and curators are load-bearing, not optional — so Tier 3 growth is
explicitly gated on curation capacity and measured value, never enrolled ahead
of them.

## ORG-004 — Sharing stays promotion-based; platform Instructions get code-level review

**Decision:** The scope defaults hold at org scale: **project-private by
default, promotion as a deliberate reviewed act, interfaces-first sharing.**
Two org-scale corollaries are binding:

- **Platform-scope Instructions are org-wide configuration.** A standing rule
  injected into every agent across 10 teams is effectively a production
  change; changes route through the promotion-approval surface
  (ADV-CONSOLE-002) with code-review rigor.
- **Cross-team contradictions need a human owner.** The Uncertainty queue
  captures disagreement between teams; a named resolver role (not "whoever
  notices") decides. Assigning that role is an open operational question for
  first multi-team enrollment.

**Why:** the failure mode at 10 teams is not technical. Over-sharing by
default produces noise, political friction, and disengagement — teams
sanitize what they write, which destroys the memory's value. Trust is built
by private-by-default plus a visible audit trail, both already in the
architecture.

## Consequences

- `arrive-sync` grows toward Tier 1 doc ingestion (status + advance linkage)
  before any contract or code-adjacent ingestion is attempted.
- A future contracts-ingestion advance implements Tier 2 (verbatim interface
  storage with SHA provenance pointers).
- Codemap (Solution Intent §6) remains deferred; when it lands it is Tier 2
  metadata (change profile), still never source bodies.
- Multi-team rollout adds an operational prerequisite: named Uncertainty
  resolver + curation capacity, tracked as enrollment gates rather than
  feature work.

## Revisit triggers

- Evidence that summary-plus-pointer retrieval fails consumers who genuinely
  needed source bodies in-context.
- Tier 1/2 value metrics healthy and teams requesting Tier 3 faster than
  curation staffing grows (forces the gate question explicitly).
- A harness ecosystem shift (e.g. agents with native org-wide code search)
  that changes what Totem uniquely needs to hold.
