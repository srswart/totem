# Totem — Solution Intent

**Status:** Draft for review · **Date:** 2026-08-05 · **Companion:** [project-brief.md](project-brief.md)

This document describes the intended shape of the solution in enough detail to
derive ARRIVE artifacts (system, components, advances). It states current intent;
variable sections are marked as such and are expected to be firmed up by early
advances.

## 1. Architecture overview

Totem is a Rust workspace deploying as one service plus a web console:

```
┌─────────────┐  ┌──────────────┐  ┌───────────────┐
│ Claude Code  │  │ Cursor (local │  │ Cloud agents   │
│ (desktop)    │  │ + background) │  │ (Anthropic, …) │
└──────┬──────┘  └──────┬───────┘  └───────┬───────┘
       │ MCP stdio       │ MCP             │ MCP streamable HTTP (authd)
       ▼                 ▼                 ▼
┌──────────────────────────────────────────────────┐     ┌──────────────┐
│ totem-gateway  (MCP server + Axum REST API)      │◄────┤ totem-console │
├──────────────────────────────────────────────────┤     │ (Dioxus)      │
│ totem-core     (domain: memory, scopes, ledger)  │     └──────────────┘
├──────────────────────────────────────────────────┤
│ totem-store    (SurrealDB schema + repositories) │
└───────────────────────┬──────────────────────────┘
                        ▼
                 ┌────────────┐      ┌───────────────────┐
                 │ SurrealDB   │◄─────┤ totem-arrive-sync  │◄── enrolled repos
                 └────────────┘      │ (ingest /arrive/)  │    (git / CLI hook)
                                     └───────────────────┘
                 AI curators run against the same core API
```

Every agent turn follows **read → think → write**: one SurrealQL round trip
assembles graph traversal + vector search + temporal facts into complete context;
the agent reasons; results persist in one ACID transaction — decisions, entity
updates, and triggered events commit together or not at all.

## 2. Domain model

### 2.1 Typed memory

Each memory is one record of exactly one category. Categories differ in
lifecycle and retrieval weight — this is the core of "typed, not a blob":

| Category | Holds | Lifecycle |
|---|---|---|
| **Episodic** | Every session and turn, kept exactly as it happened | Append-only, never edited; the audit substrate |
| **Identity** | People, agents, systems in play and what's true about them | Long-lived, curated |
| **Knowledge** | Facts and preferences about the domain | Long-lived, refined over time, decays without reinforcement |
| **Context** | The working set — what's going on right now | Short TTL, replaced fast |
| **Instructions** | Standing rules — how this team/project wants things done | Long-lived, human-reviewable, highest injection priority |
| **Uncertainty** | Contradictions kept explicit instead of silently resolved | Lives until a human or curator resolves it, with the resolution recorded |

Common record shape (variable — to be finalized in the schema advance):

- identity: `id`, `category`, `scope`, `subject` (graph link to entity/component/advance)
- content: `body`, `embedding` (vector), `tags`
- provenance: `author` (human | agent | curator), `harness`, `session`, `turn`,
  `created_at`, `derived_from` (graph links to episodic source)
- economics: `use_count`, `last_used_at`, `value_score`, `currency` (freshness/decay)
- governance: `status` (active | contested | retired), `review` state

### 2.2 Scopes — isolation and sharing

Scope is a first-class field enforced in `totem-store` (an invariant, not an
application-layer courtesy):

- `actor:<id>` — private to one developer or one agent identity
- `project:<repo>` — shared by everyone working an enrolled repo
- `team:<id>` — cross-project team conventions
- `platform` — the shared landscape all enrolled actors see

Sharing is by **promotion**, not by default: a memory written at `actor` scope
can be proposed for `project`/`platform` scope; promotion is a recorded event
subject to policy (auto for low-risk categories like Knowledge, human-gated for
Instructions). Reads always resolve the scope chain (actor → project → team →
platform) with dedup and precedence, so callers get one merged, relevant view.

### 2.3 ARRIVE landscape

Totem mirrors the ARRIVE artifacts of every enrolled repo as graph entities:

- `repo` → `system` → `component` (with stage: incubating/candidate/resident)
- `advance` (planned / in-progress / done) linked to components, PRs, evidence
- edges: `impacts`, `depends_on`, `owned_by`, plus links from memories to the
  artifacts they concern

`/arrive/` in each repo stays authoritative; `totem-arrive-sync` ingests changes
(git-triggered or CLI-hooked) and records sync provenance. The landscape answers,
in one query: what is this project made of, what's in flight, what just finished,
and what does the team already know about each part.

## 3. Interfaces

### 3.1 MCP (primary agent standard)

Served by `totem-gateway` over stdio (desktop) and streamable HTTP with token
auth (cloud — Cursor background agents, Anthropic cloud sessions, others as
their MCP support allows). Initial tool surface (names variable):

- `totem_recall(query, scope?, categories?)` — merged, ranked context in one call
- `totem_save(category, body, subject?, scope?)` — write with provenance auto-attached
- `totem_landscape(repo?, filter?)` — ARRIVE systems/components/advances view
- `totem_advance_status(advance_id)` / `totem_advance_log(...)` — process-attuned
  read/write for the active advance
- `totem_feedback(memory_id, signal)` — explicit value signal (used / wrong / stale)
- `totem_contest(memory_id, reason)` — file an Uncertainty record instead of
  overwriting

### 3.2 REST API (Axum)

Same operations plus admin/governance endpoints (audit queries, promotion
approvals, curator controls, metering reports) for the console and tooling.
SurrealDB **live queries** feed console updates in real time.

### 3.3 Enrollment

A repo enrolls via CLI (`totem enroll`, wrapping API calls): registers the repo,
performs initial ARRIVE ingestion, and installs the sync hook. An actor (human or
agent) enrolls by obtaining a scoped credential; cloud agents get least-privilege
tokens bound to repo + scope.

## 4. Audit, metering, and the value loop

- **Audit:** every read and write appends to an access log (who, what, when,
  via which tool, in which session). The Episodic category doubles as the raw
  substrate — curation and promotion actions reference the episodes they derive
  from, so any memory's lineage is reconstructable.
- **Metering:** per-memory counters (retrievals, injections, citations) and
  per-actor/per-harness usage aggregates.
- **Value and currency:** each memory carries a `value_score` (updated from
  explicit `totem_feedback` and implicit signals — was a retrieved memory
  actually used in the turn that followed?) and a `currency` term that decays
  with time and refreshes on reinforcement. Retrieval ranking = relevance
  (vector + graph proximity) × value × currency, weighted per category.
  Low-value, low-currency memory sinks and eventually surfaces in the console's
  "retire?" queue rather than disappearing silently.

## 5. Humans observe; AI manages

- **Console (Dioxus):** landscape dashboard (systems/components/advances across
  enrolled repos), memory browser by scope and category, audit trail viewer,
  contested-memory (Uncertainty) queue, promotion approvals, value/usage reports.
- **AI curators:** background agents using the same core API for deduplication,
  consolidation (merging near-duplicate Knowledge), decay processing, and
  contradiction detection (auto-filing Uncertainty records). Curator actions are
  never destructive: originals are superseded, not deleted; every action is
  logged and reversible from the console.

## 6. Deferred: Codemap

A candidate later phase maintains a representation of each enrolled codebase and
its change profile: component-level code maps, churn hotspots, ownership signals,
linked to advances that touched them. Deliberately out of the initial scope; the
graph model reserves room for it (`component` entities can grow `code_unit`
children). Decision point: after the landscape + value loop prove out.

## 7. Technology commitments

- **Backend:** Rust, Axum, SurrealDB (embedded for local/dev, server for shared
  deployment). MCP via the official Rust SDK (`rmcp`) — verify current state at
  implementation time.
- **Frontend:** Dioxus (web target first; desktop shell optional later).
- **Workspace layout (intended):** `crates/totem-core`, `crates/totem-store`,
  `crates/totem-gateway`, `crates/totem-arrive-sync`, `crates/totem-curator`,
  `crates/totem-cli`, `crates/totem-console`.

## 8. Proposed ARRIVE decomposition

System: `058-totem-core` (exists). Roots should widen from `src/**` to
`crates/**` when the workspace lands. Proposed components (all start
`incubating`; ids drive advance ids `ADV-<COMPONENT>-<SEQ>`):

| Component id | Type | Covers |
|---|---|---|
| `core` | library | Domain model: typed memory, scopes, provenance, value ledger |
| `store` | data-contract | SurrealDB schema, migrations, repositories, scope invariants |
| `gateway` | service | MCP server + Axum REST, auth, enrollment |
| `arrive-sync` | service | Ingestion of `/arrive/` artifacts, landscape graph |
| `curator` | service | AI curation jobs: dedupe, consolidation, decay, contradiction detection |
| `console` | ui-area | Dioxus frontend: dashboards, audit, review queues |
| `cli` | library | `totem` CLI: enroll, sync hook, local admin |

Key invariants to record on components:

- `store`: scope isolation enforced at the store layer; episodic records are
  append-only; every write carries provenance.
- `curator`: curation never deletes — supersede + log + reversible.
- `gateway`: cloud credentials are least-privilege (repo + scope bound).

### Candidate advance roadmap (phase 1, one reviewable advance each)

1. `ADV-CORE-00x` — workspace scaffold + domain types (memory categories, scopes, provenance)
2. `ADV-STORE-001` — SurrealDB schema + repositories + scope-isolation tests
3. `ADV-GATEWAY-001` — Axum API: recall/save with provenance + access log
4. `ADV-GATEWAY-002` — MCP server (stdio) exposing recall/save/landscape
5. `ADV-ARRIVE-SYNC-001` — ingest this repo's own `/arrive/` into the landscape graph (dogfood)
6. `ADV-CONSOLE-001` — landscape dashboard + memory browser (read-only)
7. `ADV-CORE-00x` — metering + value/currency scoring in retrieval ranking
8. `ADV-GATEWAY-003` — streamable-HTTP MCP + auth for cloud agents
9. `ADV-CURATOR-001` — first curator job (dedupe) with supersede/rollback
10. `ADV-CONSOLE-002` — audit trails, Uncertainty queue, promotion approvals

Phase 2 candidates: team scope policies, cross-repo platform views, Codemap spike.

## 9. Open questions

- **Deployment topology:** one shared Totem per team/platform vs. per-developer
  instances that sync — start shared-single-instance; revisit if offline use matters.
- **Embedding provider** for vector search (local model vs. API) and where
  embedding happens (gateway vs. curator).
- **Value attribution depth:** how far to chase "did this memory change the
  outcome?" beyond citation signals in v1.
- **Cloud harness reach:** which cloud agents can actually attach remote MCP
  today (verify per-harness at gateway build time).
- **Identity/authn provider** for humans on the console (likely defer to
  reverse-proxy/SSO initially).
