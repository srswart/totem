# Dogfood Plan: Totem as the memory layer for building Totem

**Status:** DRAFT for discussion (2026-08-07) — becomes real via the advances
in §4 once reviewed. **Companion:** docs/tech-direction/deployment.md
(DEP-001), docs/tech-direction/mcp.md, docs/overnight-experiment/log.md.

## 1. The goal

Every session that builds Totem — the hourly cloud routines, workstation
Claude Code sessions, eventually Cursor — attaches to a shared, durable,
authenticated Totem instance and uses it for real: `totem_recall` at session
start, `totem_save` for decisions and learnings, `totem_feedback`/`totem_contest`
closing the value loop, the landscape staying live via the sync hook. The
overnight experiment's journey reports and observation log keep their roles as
*curated* records; Totem becomes the *working* memory underneath them.

This is also the strongest possible test of the product: if Totem doesn't
make Totem's own development better, that is a finding (gaps-doc item 7's
"does Totem actually help" experiment gets its measurement here).

## 2. What already exists (end of phase-007)

Durable single-owner gateway (DEP-001, `TOTEM_DATA_DIR` + RocksDB); bearer
credentials bound repo+scope with fail-closed verification; streamable-HTTP
MCP at `/mcp` with the full tool surface (recall/save/landscape/feedback/
contest/advance_status/advance_log); CLI enroll + sync hook; console with
governance views; offline snapshot backup/restore; phase-008 (in flight)
adds repo-binding (GATEWAY-009), auth-refusal logging (CORE-006), the live
console relay (CONSOLE-003), and the evaluation harness.

## 3. Gap analysis — what dogfooding actually requires

### 3.1 A reachable instance (the big one)

The gateway runs on a workstation loopback today. Cloud routines run in
Anthropic's sandbox; the sanctioned path for them to reach external tools is
a **claude.ai MCP connector attached to the routine** (the same mechanism the
routines already use for other connectors) — which requires the gateway's
`/mcp` to be a public **HTTPS** endpoint. That means:

- **Hosting.** DEP-001's single instance needs a machine that is always on.
  Options, in rough order of preference: (a) a small always-on cloud host
  (container or VPS) running the durable gateway; (b) the workstation plus a
  tunnel (Tailscale Funnel / cloudflared) — zero new hosting but couples the
  team's memory to one laptop's lid being open. Decision needed (§5).
- **TLS + domain.** The gateway serves plain HTTP; DEP-001 scoped TLS out.
  Standard answer: a reverse proxy (Caddy) terminating TLS in front of the
  loopback gateway, on a small domain. The single-owner invariant is
  unaffected — the proxy is a client, not a store owner.
- **Probe first.** docs/tech-direction/mcp.md verified the transport
  end-to-end in-sandbox but could NOT verify each harness's outbound
  connector reach from primary sources. Before building anything: attach a
  trivial HTTPS MCP endpoint as a connector to a test routine and prove a
  cloud run can call one tool through it. One afternoon; de-risks the whole
  plan.

### 3.2 Credentials that survive and identities that mean something

- **Durable credential registry.** `TokenRegistry` is an in-memory map;
  every grant except the env-seeded bootstrap vanishes on restart. Dogfood
  needs store-backed credentials (issued, listed, revoked against the
  durable store — fingerprints only, per the existing invariant).
- **Identity model for agents.** Proposal: one actor per routine
  (`cloud-opus`, `cloud-sonnet`), `shawn` for workstation sessions; all
  bound to repo `srswart/totem`. Routine credentials project-scoped (they
  need shared project memory); promotion to platform stays human-approved.
  Actor-scoped memories then give each routine a private lane, and the
  access log distinguishes who knew what — which the overnight analysis
  will want.
- **Issuance/rotation runbook.** `totem credential issue` exists (local
  file); wiring it to the durable registry and writing the rotation
  procedure is part of the registry advance.

### 3.3 Harness wiring

- **Cloud routines:** attach the Totem MCP connector (with per-routine
  bearer) to both routine configs; extend `docs/cloud-agent-notes.md` with a
  memory discipline — Step 3.5 "recall before reading docs" (query the
  advance id + components you're about to touch), Step 9.5 "save what the
  next run needs" (decisions made, dead ends hit, corrections to specs —
  *not* a dump of the diff), `totem_advance_log` on completion,
  `totem_feedback` when a recalled memory actually helped or misled.
- **Workstation Claude Code:** register the gateway MCP
  (`claude mcp add --transport http`), and add the same memory discipline to
  `CLAUDE.local.md` (ARRIVE never overwrites it). The assistant's private
  file-based memory stays for assistant-personal context; project memory
  moves to Totem where every session shares it.
- **Sync hook:** `totem enroll` on the deployed instance; the post-commit
  hook keeps the landscape live from the workstation. Cloud runs can't reach
  localhost — landscape freshness from cloud merges arrives via the next
  workstation sync (accepted) or a later CI-side sync step (deferred).

### 3.4 Quality floor before we rely on it

- **Real embedder.** The deployed gateway currently ships
  `DeterministicEmbedder` (non-semantic); recall quality for dogfooding
  needs BGE-small-en-v1.5 behind the store's `fastembed` feature (EMB-004
  pinned it; the hosted build can fetch the model where the sandbox
  cannot). Re-embedding existing rows is part of the cutover.
- **Security before exposure.** A public HTTPS `/mcp` holding our working
  memory should not precede the phase-008 hardening pair (GATEWAY-009,
  CORE-006 — both already dependencies of the evaluation) and ideally not
  precede ADV-GATEWAY-006 (the security evaluation, phase-009). Proposed
  gate: **deploy privately (tunnel/allowlist) after phase-008; go public
  after GATEWAY-006 passes.**
- **Operational loop.** Scheduled backups of the data dir (launchd/cron +
  `infra/backup.sh`, offsite copy); a schedule for the curator dedupe job
  (CURATOR-001 built the job, nothing runs it); gateway restart-on-failure
  supervision. Totem holding the team's memory makes "backup deserves
  first-class treatment" (gaps doc) literal.

## 4. Proposed advances (draft decomposition)

| # | Advance (suggested id) | Scope | Where |
|---|---|---|---|
| 1 | ADV-GATEWAY-011 — connector reach probe | Prove a cloud routine can call a tool on an external HTTPS MCP connector; record per-harness findings in mcp.md | WORKSTATION (needs routine config + a throwaway endpoint) |
| 2 | ADV-GATEWAY-012 — durable credential registry | Store-backed TokenRegistry (issue/list/revoke vs the durable store), rotation runbook | cloud-eligible |
| 3 | ADV-INFRA-002 — deployable image + TLS | Container image for the durable gateway, Caddy TLS termination, compose/launchd packaging | WORKSTATION |
| 4 | ADV-INFRA-003 — operational loop | Scheduled backups (+offsite), curator job scheduling, restart supervision, minimal uptime/health check | WORKSTATION |
| 5 | ADV-STORE-008 — real embedder in deployment | `fastembed` feature in the deployed build; re-embed existing rows; recall-quality smoke against EMB-004's golden queries | WORKSTATION |
| 6 | ADV-DOGFOOD-001 — cutover | Enroll the deployed instance, issue routine + workstation credentials, wire both harnesses, update cloud-agent-notes.md with the memory discipline, define the measurement (recall/save counts, feedback ratio, "did a memory change a run" journal line in journey reports) | WORKSTATION |

Sequencing: 1 can run **now** (pure probe, parallel to phase-008). 2 is
cloud-eligible and could slot into phase-008's tail or phase-009. 3–6 are the
workstation track, in order, gated as §3.4 proposes. Console auth
(ADV-GATEWAY-010, reserved) improves the human side but does not block agent
dogfooding.

## 5. Open decisions (Shawn)

1. **Hosting:** small always-on cloud host vs workstation + tunnel. (Cost vs
   coupling; the memory estate lives wherever this lands.)
2. **Exposure gate:** agree private-after-phase-008 / public-after-GATEWAY-006,
   or accept earlier public exposure with strong tokens?
3. **Identity model:** per-routine actors as proposed, or something finer
   (per-advance? per-session)?
4. **Measurement:** is the §4.6 measurement set enough to answer "does Totem
   actually help" for the twin-experiment question, or should we design that
   properly first?
