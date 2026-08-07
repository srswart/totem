# Dogfood Plan: Totem as the memory layer for building Totem

**Status:** DECIDED 2026-08-07 (Shawn) — hosting on a small always-on cloud
host; the trial's data is trial-grade (preservation nice-to-have, not
critical); access stays secure without over-complication (TLS + bearer
credentials + the phase-008 hardening pair; the public-only-after-GATEWAY-006
gate from the draft is relaxed accordingly). Realized via the advances in §4
(now authored into phase-010). **Companion:** docs/tech-direction/deployment.md
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
- **Probe executed 2026-08-07 (ADV-GATEWAY-011) — and it changed the plan.**
  claude.ai connectors are **OAuth-only**: no static bearer can be
  configured, and Dynamic Client Registration against Totem fails
  (MCP-013). **Resolved 2026-08-07 by reading the spec: Totem implements
  only the OAuth 2.1 *resource server* role** (ADV-GATEWAY-013) — RFC 9728
  metadata, a discovery-friendly 401, and third-party token validation with
  audience binding. The authorization server is a third-party identity
  provider (explicitly out of scope per the MCP spec); Totem issues no OAuth
  tokens and runs no login UI. This supersedes the interim "OAuth proxy
  front" recommendation — keeping authorization in the gateway keeps it
  where the scope invariants already live. Totem's own repo+scope+actor
  grants stay as they are; the OAuth path maps onto them. Also: the OAuth discovery documents must sit outside the
  auth layer (MCP-014), and public hostnames must be named via
  `TOTEM_MCP_ALLOWED_HOSTS` (MCP-012, shipped).

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

| # | Advance | Scope | Where |
|---|---|---|---|
| 1 | ADV-GATEWAY-011 — connector reach probe | Prove a cloud routine can call a tool on an external HTTPS MCP connector; findings into mcp.md | WORKSTATION |
| 2 | ADV-GATEWAY-012 — durable credential registry | Store-backed grants, privileged issuance endpoint, rotation runbook | cloud-eligible |
| 3 | ADV-INFRA-002 — hosted deployment | Container image + Caddy TLS + supervision + daily backup + estate migration (trial-grade ops folded in) | WORKSTATION |
| 4 | ADV-STORE-008 — real embedder in deployment | fastembed/BGE in the hosted build; re-embed; golden-query smoke | WORKSTATION |
| 5 | ADV-INFRA-003 — cutover | Enroll, per-identity credentials, connector + harness wiring, memory discipline, measurement | WORKSTATION |

All five are authored into **phase-010 ("Dogfood trial")**, sequenced before
the evaluations phase (now order 3) so the trial starts as soon as the
workstation track completes; the security evaluation then runs against the
deployed, dogfooding system.

Sequencing: 1 can run **now** (pure probe, parallel to phase-008). 2 is
cloud-eligible and could slot into phase-008's tail or phase-009. 3–5 are the
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
4. **Measurement:** is the ADV-INFRA-003 measurement set enough to answer "does Totem
   actually help" for the twin-experiment question, or should we design that
   properly first?
