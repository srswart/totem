# Totem — Project Brief

**Status:** Draft for review · **Date:** 2026-08-05 · **Repo:** 058-totem

## One-liner

Totem is a durable, auditable memory and shared-context system for AI agents — a
typed context layer beneath every harness (Claude Code, Cursor, cloud agents) that
knows the full landscape of every enrolled project, including its ARRIVE systems,
components, and advances.

## Problem

Every AI harness today rebuilds its short-term memory from scratch: the context
window lasts one turn, then it's gone. What persists is fragmented per-tool
(Claude Code memory files, Cursor rules, CLAUDE.md), per-developer, and per-machine.
The consequences:

- **No shared landscape.** A cloud agent working a PR and a developer's local agent
  working the same repo have no common view of what's in flight, what's done, or
  what's been decided.
- **No accountability.** Memory that agents act on is not auditable — nobody can
  answer "why did the agent believe that?" or "where did this instruction come from?"
- **No feedback loop.** There is no measure of which remembered facts are actually
  used, which are stale, and which earn their retrieval weight.
- **Process blindness.** Our ARRIVE governance (systems, components, advances) is
  the authoritative record of intent and progress, but no agent memory today is
  attuned to it.

## Vision

A single context layer — Totem — that any enrolled project and any harness plugs
into through standard mechanisms (MCP first, HTTP API alongside). Totem holds:

1. **Typed memory, not a blob.** Six categories, each with its own lifecycle and
   retrieval weight: Episodic (the raw record), Identity (who's who), Knowledge
   (facts and preferences), Context (the working set, short-lived), Instructions
   (standing rules), and Uncertainty (contradictions kept explicit rather than
   silently resolved).
2. **Isolation and sharing by design.** Memory is scoped (actor-private, project,
   team, platform-shared). Sharing is deliberate and policy-governed: relevant
   context flows to every developer and every agent; private context stays private.
3. **The ARRIVE landscape.** For every enrolled repo, Totem maintains a live view
   of systems, components, and advances — planned, in progress, and done — so any
   agent starts a session already knowing the state of the work.
4. **Audit, measurement, and value.** Every read and write is recorded with
   provenance. Usage is metered per memory. A feedback loop captures a
   representation of value and currency (freshness), so retrieval favors memory
   that earns its keep, and stale or low-value memory decays visibly rather than
   silently.
5. **Humans observe; AI manages.** A human-facing console exposes the memory
   estate for review and governance. The complex interior — consolidation,
   deduplication, contradiction detection, decay — is delegated to AI curators,
   but every curator action leaves an auditable trail a human can inspect and
   reverse.

## Goals

- G1: Any enrolled harness (desktop or cloud) can read relevant shared context and
  write back memories through a standard interface with near-zero setup.
- G2: The full ARRIVE landscape of every enrolled project is queryable in one
  round trip — one multi-model query returning documents, relations, vectors, and
  history together.
- G3: Every memory carries provenance (who/what/when/from-which-turn) and every
  access is audit-logged.
- G4: Usage and value metrics exist per memory and drive retrieval weighting.
- G5: Humans can browse, review, correct, and retire memory through a console;
  AI curation runs continuously with reversible, logged actions.

## Non-goals (initial releases)

- Not a general RAG platform over arbitrary documents — Totem is agent memory and
  process context, not a document search product.
- Not a replacement for git or for ARRIVE artifacts — `/arrive/` in each repo
  remains authoritative; Totem mirrors and indexes it.
- Not multi-tenant SaaS hardening in v1 — single team/platform deployment first.
- Codebase representation and change-profile analytics are a candidate later
  phase, not a launch commitment (see Solution Intent, "Deferred: Codemap").

## Users and consumers

- **Developers** — observe and steward memory through the console; benefit from
  agents that arrive pre-briefed.
- **Local agents** — Claude Code, Cursor, and other MCP-capable harnesses on the
  desktop.
- **Cloud agents** — Cursor background agents, Anthropic cloud sessions, CI-driven
  agents — connecting over authenticated remote MCP / HTTP.
- **AI curators** — Totem's own maintenance agents (consolidation, decay,
  contradiction management).
- **Reviewers / leads** — audit trails, value reports, landscape dashboards.

## Success measures

- Time-to-context: a fresh agent session reaches "knows the landscape" in one tool
  call instead of N file reads.
- Reuse rate: fraction of retrieved memories that are actually used (cited or
  acted on) per session — the core value-loop metric.
- Staleness: median age of contradicted-but-unresolved memory trends to zero.
- Coverage: % of active repos enrolled; % of advances visible in Totem within
  minutes of change.
- Auditability: any memory's full provenance and access history reconstructable
  on demand.

## Constraints and givens

- **Stack:** Rust backend (Axum), SurrealDB as the datastore, Dioxus frontend.
  SurrealDB is chosen for its multi-model nature (document + graph + vector +
  time-series in one SurrealQL statement, ACID transactions, live queries).
- **Interfaces:** MCP (stdio for desktop, streamable HTTP for cloud) as the
  primary agent standard; REST/JSON alongside for tooling and the console.
- **Governance:** this repo is ARRIVE-governed; all work lands as reviewable
  advances under `058-totem-core`.

## Key risks

- **Sharing/isolation mistakes** — leaking private context across scopes is the
  highest-severity failure; scope enforcement must be a store-level invariant,
  not an application courtesy.
- **Curation trust** — AI-managed consolidation that silently rewrites memory
  would destroy auditability; mitigated by append-only episodic record +
  reversible curator actions.
- **Value signal quality** — usage metering is easy, value attribution is hard;
  start with simple proxies (retrieval → citation → outcome) and iterate.
- **Standard drift** — MCP and harness capabilities are moving targets; keep the
  gateway thin over a stable internal API.
