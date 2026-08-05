# Totem — ARRIVE Decomposition: Gap Analysis

**Date:** 2026-08-05 · **Inputs:** [project-brief.md](project-brief.md), [solution-intent.md](solution-intent.md)
**Companion to:** the generated artifacts under `arrive/systems/058-totem-core/`

This document records where the Project Brief / Solution Intent describe
behavior that the proposed decomposition (Solution Intent §8) did not assign to
any advance, and what was done about each gap.

## What was generated

- **System** `058-totem-core` — roots widened to include `crates/**` for the
  intended workspace layout (Solution Intent §7). The placeholder `src/**` root
  was dropped in ADV-CORE-001 when the workspace landed.
- **Components** (all `incubating`, per §8): `core`, `store`, `gateway`,
  `arrive-sync`, `curator`, `console`, `cli` — with the key invariants from §8
  recorded on `store`, `curator`, `gateway`, plus the mirror-only invariant on
  `arrive-sync`.
- **Advances**: the ten roadmap advances from §8 as `planned`
  (ADV-CORE-001/002, ADV-STORE-001, ADV-GATEWAY-001/002/003,
  ADV-ARRIVE-SYNC-001, ADV-CONSOLE-001/002, ADV-CURATOR-001), plus four
  gap-fill advances below, plus the nine profiled robustness advances
  (investigation / evaluation / enablement) in the section after that.
  All advances declare schema v2 outcome profiles (`mode`, `facets`,
  `work_products`).

## Gaps filled with new planned advances

| Gap | Where described | Why the roadmap missed it | Created |
|---|---|---|---|
| **Embedding pipeline + vector index.** Recall ranking = relevance (vector + graph) × value × currency, and every record carries an `embedding` — but no advance generates embeddings or builds the index. Provider/placement is an open question (§9). | §2.1, §4, §9 | Implied by "recall" but never assigned | `ADV-STORE-002` |
| **Enrollment CLI.** `totem enroll` (register + initial ingestion + sync hook) and actor credential issuance are a core flow; the `cli` component had no advance at all. | §3.3 | Component listed, no advance | `ADV-CLI-001` |
| **Half the MCP tool surface.** `totem_feedback`, `totem_contest`, `totem_advance_status`, `totem_advance_log` are named tools; ADV-GATEWAY-002 covers only recall/save/landscape. Without `totem_feedback` the value loop (G4) has no explicit input; without `totem_contest` the Uncertainty category has no write path from agents. | §3.1, §4 | Roadmap items 4/8 cover transports, not these tools | `ADV-GATEWAY-004` |
| **Scope promotion mechanics.** "Sharing is by promotion" with per-category policy (auto for Knowledge, human-gated for Instructions) is a pillar; ADV-CONSOLE-002 builds the approval *UI* but nothing builds the propose/approve/demote event engine it fronts. | §2.2 | UI assigned, engine unassigned | `ADV-CORE-003` |

## Robustness: experiments, evaluations, and their enablement

The source docs contain experiments-in-waiting (open questions, "verify at
implementation time" notes, load-bearing assumptions) and imply evaluations
(key risks, success measures) without assigning either to advances. These are
now first-class, with proper outcome profiles:

**Investigation advances** (experiments; findings are the deliverable):

| Advance | Experiment | Feeds |
|---|---|---|
| `ADV-STORE-003` | Embedding provider + placement spike (§9 open question) | ADV-STORE-002 |
| `ADV-STORE-004` | SurrealDB multi-model one-round-trip + ACID + live-query spike — validates the load-bearing assumption behind G2 and the turn model (§1) | ADV-STORE-001 |
| `ADV-GATEWAY-005` | `rmcp` maturity + per-harness remote-MCP capability matrix (§7, §9) | ADV-GATEWAY-002/003 |
| `ADV-CORE-004` | Value-attribution proxy experiment (§9; named key risk) | ADV-CORE-002 |

**Enablement advances** (coding required for evaluation; no TDD claims —
practices resolve from work products):

| Advance | Work products | Provides |
|---|---|---|
| `ADV-STORE-005` | `test_data` | Synthetic memory corpus: all categories × scopes, golden queries, leak-bait and contradiction fixtures; deterministic seed/reset |
| `ADV-GATEWAY-008` | `performance_harness`, `test_automation` | Workload driver (with environment stamping) + recall-quality scorer, both proven with negative/positive controls |

**Evaluation advances** (one per facet the user cares about):

| Advance | Facet | Evaluates |
|---|---|---|
| `ADV-CORE-005` | quality | Recall relevance + ranking behavior vs. golden corpus; value loop on/off comparison |
| `ADV-GATEWAY-006` | security | Adversarial scope-isolation/auth/promotion evaluation — the brief's highest-severity failure class; threat model + findings disposition required |
| `ADV-GATEWAY-007` | performance | Time-to-context and recall/save latency under load; establishes the comparison baseline |

## Gaps documented but not yet materialized

These need either a decision first or belong after phase 1. Suggested ids are
reserved here so they can be created when ready.

1. **Deployment & operations — suggest a new `infra` component** (type
   `infra`) **+ `ADV-INFRA-001`.** Nothing covers service packaging, SurrealDB
   server deployment (embedded vs. server split, §7), configuration, backup /
   restore of the memory estate, or observability of Totem itself. Blocked on
   the deployment-topology open question (§9: start shared single instance).
   Note the irony risk: Totem is the audit system — losing its data loses the
   team's memory; backup deserves first-class treatment.
2. **Remaining curator jobs — `ADV-CURATOR-002` (decay processing),
   `ADV-CURATOR-003` (contradiction detection → auto-file Uncertainty),
   `ADV-CURATOR-004` (consolidation).** §5 names four curator capabilities;
   only dedupe is roadmapped. Decay processing is the most urgent of the
   three: ADV-CORE-002 defines the currency/decay *math*, but no scheduled
   process *executes* decay or expires short-TTL Context records.
3. **Episodic retention.** Episodic is append-only and records *every* session
   and turn; unbounded growth is unaddressed. Needs a retention/archival
   policy (archive ≠ delete, to preserve the audit invariant) — likely folds
   into the `infra` component or a store advance.
4. **Continuous multi-repo sync — `ADV-ARRIVE-SYNC-002`.** ADV-ARRIVE-SYNC-001
   dogfoods on this repo; the coverage success measure ("advances visible
   within minutes") needs incremental, hook-triggered sync for *other*
   enrolled repos, plus failure/drift handling (hook didn't fire, artifacts
   invalid, force-push history rewrite).
5. **Console human authentication.** Open question (§9) — likely
   reverse-proxy/SSO initially. When decided, record it and implement under
   `gateway` (suggest `ADV-GATEWAY-009`) or the `infra` component.
6. **Tech-direction records.** CLAUDE.md expects major decisions in
   `docs/tech-direction/`, which doesn't exist yet. The decisions already made
   or pending deserve entries: SurrealDB choice (made, §7), embedding provider
   (pending, ADV-STORE-002), deployment topology (pending), value-attribution
   depth (pending, ADV-CORE-002).

## Explicitly deferred — not gaps

Named as later-phase in the source docs, so intentionally not decomposed:
team scope policies, cross-repo platform views, and the Codemap
(component-level code maps / churn; the graph model reserves `code_unit`
children for it). Multi-tenant SaaS hardening is a stated non-goal for v1.

## Suggested sequencing note

The §8 roadmap order holds, with the gap-fills slotting in as:
ADV-STORE-002 (embeddings) before or with ADV-GATEWAY-001 (recall needs it to
rank); ADV-CLI-001 after ADV-ARRIVE-SYNC-001 (enroll wraps ingestion);
ADV-GATEWAY-004 with ADV-CORE-002 (feedback feeds the value loop);
ADV-CORE-003 before ADV-CONSOLE-002 (the approval UI fronts the engine).

The profiled advances slot in around that spine:

- **Investigations run earliest** — ADV-STORE-004 (SurrealDB spike) before
  ADV-STORE-001 commits to a schema; ADV-STORE-003 (embeddings) before
  ADV-STORE-002; ADV-GATEWAY-005 (MCP spike) before ADV-GATEWAY-002;
  ADV-CORE-004 (value proxies) before ADV-CORE-002.
- **Enablement precedes evaluation** — ADV-STORE-005 (corpus) once the store
  exists; ADV-GATEWAY-008 (harness) once gateway endpoints exist.
- **Evaluations gate maturity** — ADV-GATEWAY-006 (security) after
  ADV-GATEWAY-003 + ADV-CORE-003, and it should be a prerequisite for
  promoting `store`/`gateway` from incubating to candidate; ADV-CORE-005
  (quality) and ADV-GATEWAY-007 (performance) after the value loop lands,
  with GATEWAY-007's first run recorded as the standing baseline.
