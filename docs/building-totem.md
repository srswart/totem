# Building Totem — the story so far

**Status:** Living document · **Last updated:** 2026-08-07 (through phase-007;
21 of 30 planned MVP advances executed) · **Primary sources:** the per-advance
agent journey reports in [docs/agent-journey/](agent-journey/) · **Secondary
sources:** advance records under `arrive/systems/058-totem-core/advances/`,
[tech-direction](tech-direction/), the
[overnight experiment log](overnight-experiment/log.md), PR history, and the
code itself.

> **How this document is maintained.** Each entry cites the journey report or
> record it derives from; nothing here is asserted from memory. It is updated
> at phase boundaries. Totem is not yet connected to any active project — this
> repo will be its first enrollment for daily development. When that happens,
> the stated ambition is for Totem to generate the successor to this document
> from its own episodic memory, and the comparison between that generated
> account and this hand-written one becomes a test of the product itself.

---

## 1. What this is the story of

Totem is a typed, scoped, audited memory system for AI agents — and it is also
an experiment in how software gets built when scheduled cloud agents execute
pre-specified work at night under ARRIVE governance, and staff review, decide,
and write runway during the day. The two stories are inseparable: nearly every
line of Totem's code was written by a cloud agent following the protocol in
[cloud-agent-notes.md](cloud-agent-notes.md), each run leaving a journey
report behind. Those reports are the primary source for everything below.

The construction so far spans, remarkably, about **48 hours of calendar time**:
the project brief and solution intent were written on 2026-08-05, and by the
evening of 2026-08-06 all seven components had shipped their core advances,
the system had run end-to-end in a live demo, and phase-007 (auth, promotion,
curation, governance console) had merged.

## 2. Setting the stage (2026-08-05, morning)

The repo began as governance before it was code: an ARRIVE skeleton, then a
[project brief](project-brief.md) and [solution intent](solution-intent.md)
written to be decomposable — seven components (`core`, `store`, `gateway`,
`arrive-sync`, `curator`, `console`, `cli`) and a ten-advance phase-1 roadmap
that later grew into a 30-advance, nine-phase implementation plan.

Two pieces of infrastructure mattered more than they looked:

- **CI triggered on `advance/**` branches** (ported from project 057), so an
  implementing agent gets real CI results from its own push — `ci:passed` as
  evidence an agent can actually obtain before any human opens a PR. Within
  hours of it landing, the first cloud agent's code went through it.
- **The cloud-agent protocol** (`docs/cloud-agent-notes.md`): a numbered
  ritual every scheduled run follows — gate on pushed state, bootstrap the
  ARRIVE CLI, read the governance context, select the next eligible advance
  (respecting model designations), implement in `tidy → test → feat` commit
  order, validate, open a PR, and write a journey report. The honesty rules in
  that protocol ("only claim `tdd:red-green` if you genuinely wrote failing
  tests first"; "say plainly what was not achieved") shaped the evidence this
  document is built from.

## 3. Day one: prove the ground before building on it (2026-08-05)

### First code, first humility

[ADV-CORE-001](agent-journey/2026-08-05T060952Z-ADV-CORE-001.md) (Opus,
PR #1) landed the workspace and the domain model — categories, scopes,
provenance, records — test-first, with 25 integration tests written against
types that did not yet exist, and mutation checks that deliberately broke both
security invariants to prove the tests would notice. Then Copilot's PR review
found two real defects anyway, and the journey report recorded the more
uncomfortable lesson verbatim: *"my own test pinned the wrong semantics"* — a
test asserting an error variant had been written from the implementation's
behaviour rather than the intended contract, so the suite defended the defect.
The second finding was subtler and worse: scope-chain precedence depended on
caller-supplied ordering, meaning two callers with identical memberships could
see different merged views. Both were fixed test-first the same day.

### Investigations before load-bearing decisions

The plan front-loaded *investigation advances* — cheap spikes answering the
questions the architecture would rest on — and day one ran four of them:

- **[ADV-STORE-004](agent-journey/2026-08-05T075629Z-ADV-STORE-004.md)**
  confirmed the load-bearing assumption of the whole design: one SurrealQL
  statement really can combine HNSW vector search, graph traversal, temporal
  cutoffs and scope filtering, with ACID writes and live queries. It also
  produced three findings that would have been expensive to discover later
  (TD-002: the vector operator variant that *silently* skips the index;
  TD-004: a string-typed datetime filters nothing, silently; TD-008: live
  notifications are unordered within a transaction — found because a test
  flaked one run in four, and the convenient fix would have buried it).
- **[ADV-STORE-006](agent-journey/2026-08-05T100656Z-ADV-STORE-006.md)** hit
  the sandbox's wall — no egress to download a SurrealDB server — and became
  the first advance executed on the developer's workstation instead. The
  payoff was the project's single most consequential finding, TD-011: a
  least-privilege database user's writes are **silently discarded** — `CREATE`
  returns OK, persists nothing. That killed the "separate database server with
  DB-level roles" topology on evidence, and later became DEP-001: one gateway
  process, embedded storage, no unlogged access path *by physical property*.
- **[ADV-STORE-003](agent-journey/2026-08-05T101500Z-ADV-STORE-003.md)** and
  **[ADV-STORE-007](agent-journey/2026-08-05T112955Z-ADV-STORE-007.md)** did
  the same dance for embeddings: the sandbox blocked every embedding-API host
  *and* model-weight downloads, so the cloud run built an offline baseline and
  labeled corpus, and a workstation session closed the question with a
  measured result — BGE-small-en-v1.5, 384 dimensions, 5/5 labeled queries
  ranked first, including the paraphrase the baseline fails.
- **[ADV-GATEWAY-005](agent-journey/2026-08-05T111200Z-ADV-GATEWAY-005.md)**
  proved the `rmcp` MCP SDK over both transports, and was honest about what a
  headless sandbox cannot verify: the Cursor row of its capability matrix is
  marked *unverified* — carried forward explicitly to the auth advance rather
  than resolved with softer language.

The pattern that emerged here — sandbox hits a wall, the work routes to a
workstation session, findings return as tech direction that later advances are
*required* to read — recurred all the way through, and eventually became a
first-class `WORKSTATION` designation in the plan.

### The store: enforcement, not convention

**[ADV-STORE-001](agent-journey/2026-08-05T185651Z-ADV-STORE-001.md)** (Opus,
PR #13) built the persistence layer against the spikes' measured behaviour —
its author ran one throwaway probe to answer five open engine questions before
writing any test, then went green on the first pass. Scope isolation was
implemented *twice* (a SQL predicate and a Rust merge filter), which produced
the run's sharpest insight: removing the SQL predicate failed exactly **one**
test — the `EXPLAIN`-plan assertion. Every result-based isolation test still
passed on the backup filter. With defence in depth, the plan assertion is not
a nicety; it is the only witness that the first line of defence still exists.

The same advance surfaced a genuine design conflict nobody had authored: the
episodic append-only `EVENT` refuses *every* update — including the
`use_count` increment the solution intent's metering required. Both could not
be true. The invariant won, the consequence was recorded (episodic metering
must live in the access log), and two later advances inherited that constraint
explicitly. This is what the journey reports call an advance "correcting
itself against reality."

Day one closed with a footnote that says everything about the machinery: a
Sonnet run picked up the next plan item, found it designated
`MODEL: claude-opus-5`, reported
[MODEL-GATED](agent-journey/2026-08-05T185832Z-ADV-STORE-001.md), and did
nothing — correctly. The plan's model designations (Opus for security-critical
and architecturally novel work, Sonnet for well-specified builds) were being
enforced by the runs themselves.

## 4. The machinery learns (overnight and 2026-08-06)

The [overnight experiment log](overnight-experiment/log.md) tracks the
process story, and its first week produced three failure modes worth retelling
— each of which failed *safe* and left a fix behind:

- **The duplicate-selection collision.** Two Sonnet sessions passed the gate
  within minutes of each other and both implemented ADV-CLI-001 for ~30
  minutes. The losing run's conduct became the template: push rejected → no
  force-push → work preserved on a `collision` ref → honest `blocked` report.
  The fix (PR #23) was *claim-first push* — an empty commit claiming the
  sub-branch before implementation, closing a 30-minute race window to
  seconds. The collision even paid for itself: two independent implementations
  of a security-relevant advance made the surviving PR's review richer.
- **The honest deadlock.**
  [ADV-CONSOLE-001](agent-journey/2026-08-06T073227Z-ADV-CONSOLE-001.md)
  shipped a working read-only console but *declined to claim completion* —
  the spec promised live auto-update, and a browserless sandbox could not
  verify rendering at all. It recorded `in_progress` with reasons, exactly as
  the honesty rules demand — and the gate had no defined behaviour for
  `in_progress`, so every subsequent hourly run deadlocked until a human made
  the scope call. The log's verdict: this is the *shape of a good failure* —
  loud, stationary, reversible. Scope decisions are day-side; the night side's
  job is to stop and say so clearly.
- **The stale plan note.** A leftover "BLOCKED on environment" note on an
  advance that had in fact executed led the *humans* to flip a done advance
  back to planned. The agents had made no error; the advance file and PR
  history were what caught it. Lesson recorded: the advance file is the
  authoritative record; plan notes rot.

Against those, the log records what worked: hourly firings idle safely as
GATED no-ops costing seconds; merge-triggered handoffs chained a whole night
of work (sub-PR merge → next advance; phase complete → the agent opens the
phase PR itself) with humans doing nothing but review and merge; and the
honesty rules produced telemetry you can actually believe — which is the only
reason aggregate analysis of this experiment, and this document, are possible.

One drift was caught by reading the reports in aggregate: the first four
implementation advances all scored Red on reviewability, and all four chose
"documented, not split" — the third citing the first as precedent. Within four
runs, "split unless documented" had operationally inverted into "document by
default." Each justification was individually reasonable (most were dominated
by `Cargo.lock` churn and scaffold, and the splits genuinely would have
shipped unenforced invariants); collectively they showed that *policy*
questions calibrate to whatever humans last accepted. Run-time agent judgment
is excellent at executing decisions and persuasive at rationalizing them —
the two need different controls.

## 5. Day two: the system takes shape (2026-08-06)

With the ground proven, the build ran fast and layered:

- **[ADV-GATEWAY-001](agent-journey/2026-08-06T034048Z-ADV-GATEWAY-001.md)**:
  the Axum gateway with `/save` and `/recall` and the append-only access log —
  and a named gap rather than a silent one: refused writes were *not* logged,
  and rather than ride an unreviewed fix inside an already-Red diff, the gap
  was documented and left for its own reviewed slice.
- **[ADV-GATEWAY-002](agent-journey/2026-08-06T043546Z-ADV-GATEWAY-002.md)**:
  the first MCP tools. The REST and MCP surfaces were refactored onto one
  shared `ops` layer so "every read and write is logged" could not drift into
  two code paths. `totem_landscape` shipped real but *honestly empty* —
  callable, schema-correct, returning empty arrays with a note naming the
  advance that would populate it, covered by a test with that exact name.
- **[ADV-ARRIVE-SYNC-001](agent-journey/2026-08-06T054432Z-ADV-ARRIVE-SYNC-001.md)**:
  the landscape mirror, proven by the project's favourite test — `dogfood.rs`
  parses and syncs *this repo's own* `/arrive/` tree (1 system, 7 components,
  24 advances at the time) into a store and reads it back. Totem's first
  enrolled project was Totem.
- **[ADV-CLI-001](agent-journey/2026-08-06T063108Z-ADV-CLI-001.md)**: `totem
  enroll` and `totem credential create` — the collision advance, above. Its
  scope note was scrupulous: it issues the *shape* of a least-privilege
  credential; nothing verifies tokens until the auth advance, and saying so
  plainly was part of the advance.
- **The value loop, in two moves.**
  [ADV-CORE-004](agent-journey/2026-08-06T101920Z-ADV-CORE-004.md) was the
  investigation, and it made an unusual choice: rather than invent synthetic
  sessions (which would calibrate to whatever proxy the author expected to
  win), it built a labeled corpus from **this repo's own two-day history** —
  17 finding→consumer pairs across TD/EMB/MCP findings, checked against the
  delivered code. Outcome-linkage scored precision 1.0/recall 0.92; citation
  (the cheap v1 proxy) 0.80/0.67; raw retrieval had *no* discriminating power
  — numeric confirmation of the brief's own stated risk. The corpus also
  surfaced a real miss in the project's history: one recorded finding
  (MCP-001's `serverInfo.name` advice) was never applied by the advance that
  should have consumed it. Then
  [ADV-CORE-002](agent-journey/2026-08-06T105137Z-ADV-CORE-002.md)
  implemented exactly the evidence-backed subset: citation-led
  `value_score`, decay-based `currency`, ranking = relevance × value ×
  currency — and explicitly did *not* build the parts the investigation found
  unmeasurable, narrowing its own objective on the record.
- **[ADV-GATEWAY-004](agent-journey/2026-08-06T115213Z-ADV-GATEWAY-004.md)**:
  `totem_feedback`, `totem_contest`, `totem_advance_status`,
  `totem_advance_log` over both surfaces — completing the seven-tool MCP
  surface the deck and docs describe.
- **[ADV-INFRA-001](agent-journey/2026-08-06T131500Z-ADV-INFRA-001.md)**: the
  first `WORKSTATION`-designated advance through the phase machinery,
  realizing DEP-001 — durable RocksDB storage with kill-and-restart and
  backup→delete→restore→restart evidence, executed with Shawn at the keyboard
  while the cloud routines correctly reported GATED within seconds of the
  claim.
- **[ADV-GATEWAY-003](agent-journey/2026-08-06T132006Z-ADV-GATEWAY-003.md)**
  (Opus): authentication — bearer credentials bounding repo, scope, and actor
  over REST and streamable-HTTP MCP. Its report contains the project's best
  argument for mutation testing as standing practice: stub both authorization
  methods to return `Ok`, and exactly the eight authorization tests fail while
  everything else passes. "That is the evidence that makes the suite credible,
  and it took two minutes." It also enumerated seven partially-established
  invariants rather than rounding up to "secure."
- **[ADV-CORE-003](agent-journey/2026-08-06T150000Z-ADV-CORE-003.md)** (Opus):
  scope promotion — the one sanctioned path across a scope boundary, modeled
  as append-only events with store-assigned ordering, the move and its event
  in one transaction. The report names "the design decision worth arguing
  about" — a reviewer must be able to *read* a record proposed for their
  scope, which is the crate's only deliberate scope-reach exception, bounded
  four ways and called out under Security Notes instead of left for a
  reviewer to discover. Promotion policy derives from the categories' own
  review policy, so a category cannot be human-gated for review and quietly
  automatic for sharing.
- **[ADV-CURATOR-001](agent-journey/2026-08-06T162543Z-ADV-CURATOR-001.md)**
  (Opus): the first curator job — semantic dedupe with supersede/rollback,
  never delete. A flaky fixture ordering became a real production fix
  (deterministic candidate ordering), and the "reversible from the console"
  invariant was flagged as only API-reversible until the console advance
  landed —
- **[ADV-CONSOLE-002](agent-journey/2026-08-06T175956Z-ADV-CONSOLE-002.md)**,
  which landed hours later: promotion approvals, the Uncertainty queue, and
  audit trails wired end-to-end into the Dioxus console, closing the loop
  three earlier advances had each left explicitly open *by name* for it.

That evening, the overnight log records the milestone plainly: first live
run — gateway up, this repo enrolled via the CLI (1 system / 7 components /
26 advances into the landscape), save/recall over REST with scope isolation
verified at the API and in the UI, and the console rendering in a real
browser for the first time.

## 6. Challenges, named

Reading across all 22 reports, the difficulties cluster:

1. **The sandbox's walls.** Egress blocks (SurrealDB binaries, every
   embedding-API host, model weights, `docs.cursor.com`), no browser, a
   ~5-minute cold SurrealDB build on every ephemeral runner, and twice a
   session death by disk exhaustion mid-link (~30 GB of incremental build
   artifacts; the fix — `cargo clean` + a single `CARGO_INCREMENTAL=0`
   sequence — is now written down for future runs). None of these stopped
   work; all of them shaped it, and the WORKSTATION lane is their permanent
   accommodation.
2. **Honesty is harder than passing.** The protocol's most distinctive
   demand — do not claim what you did not do — generated real work: stash
   tricks to reconstruct genuine red states honestly (and label them as
   reconstructions), per-module TDD disclosure, `in_progress` over false
   `complete`, evidence lists that omit `tdd:red-green` when it was not
   earned, and a "corrections made to the advance" section in nearly every
   report. The specs were wrong often enough to matter — scope too wide,
   claims that described a spike's finding rather than shipped behaviour,
   components lists missing a crate — and the discipline of correcting the
   record *in the record* is why the history can be trusted.
3. **Reviewability under a phase-scoped protocol.** Most implementation
   advances scored Red, dominated by lockfile churn and new-crate scaffold,
   and the one-advance-per-run protocol fixes scope before a run starts — so
   agents document rather than split, and the drift noted in §4 is the
   standing tension. The honest current answer: splitting is a human scoping
   decision, made when the plan is written, not an agent's at run time.
4. **Coordination at machine speed.** The collision, the deadlock, and the
   stale-note incident (§4) — all three now have codified answers
   (claim-first, defined `in_progress` handling, advance-file-is-authoritative).
5. **Tests that lie.** A test pinning the wrong error semantics; a test
   passing for the wrong reason against a pre-existing behaviour; an
   assertion that only order-insensitivity made honest; a workspace break
   invisible to per-package test runs. The countermeasure that recurs in
   every security-relevant report is mutation testing — break the guard,
   watch the right tests fail, then trust the suite.

## 7. Achievements, named

- **The system exists end-to-end**: typed memory with six categories; scope
  isolation enforced twice in the store with an `EXPLAIN` witness; provenance
  required at construction; an append-only access log, promotion trail,
  curation trail, and sync trail all guarded by the same database-level
  pattern; a citation-led value loop; auth bounding repo+scope+actor; a
  console that renders the landscape, memory, audit, Uncertainty, and
  promotion queues; and a CLI that enrolls a repo in one command.
- **Every load-bearing decision has an evidence trail**: eleven TD findings,
  four EMB findings, five MCP findings, five VAL findings, DEP-001, and
  ORG-001..004 — each traceable to the spike or session that produced it, and
  several (TD-011 above all) that reversed the default architecture before it
  was built.
- **The dogfood loop closed early**: Totem's landscape mirror has synced
  Totem's own governance tree since the first arrive-sync advance, and the
  value-attribution experiment was run against the project's own history.
- **The meta-achievement**: a two-day, ~40-PR construction in which every
  sequencing decision overnight was derived by agents from pushed state, the
  failures were loud and reversible, and the telemetry is trustworthy enough
  to write this document from. The process observations are themselves a
  deliverable — captured in the
  [overnight experiment log](overnight-experiment/log.md) with two standing
  questions (what work suits autonomous agents; what belongs to the day) and
  two open evolutions (risk-tiered auto-merge; a night-shift supervisor
  agent).

## 8. Where we are, and what remains

**Done (phases 001–007, 21 advances):** foundations, all six investigations,
store, embeddings, gateway REST+MCP, landscape sync, CLI enrollment,
read-only console, value loop v1, the four remaining MCP tools, durable
deployment, auth, promotion, curation, and the governance console.

**Remaining (phases 008–009, 9 advances):** ADV-STORE-005 (HNSW recall/`ef`
measurement at realistic corpus size), ADV-GATEWAY-008/009, ADV-CORE-006
(first-class `curate`/audit operations — already anticipated by two earlier
advances), ADV-CONSOLE-003 (live updates — the gateway-side live-query relay
the first console advance honestly declined to fake), ADV-CONSOLE-004,
ADV-GATEWAY-006, ADV-CORE-005, and ADV-GATEWAY-007 (recall performance under
the scope-index question ADV-STORE-001 deliberately deferred).

**Known open gaps carried honestly on the record:** refused requests are
authenticated but unlogged; landscape reads are authenticated but not
repo-bound; scoring constants are placeholders awaiting real telemetry;
promotion/curation double-decision races are bounded but not serialized;
console authentication remains reverse-proxy/SSO-by-assumption.

**Next milestones beyond the plan:** connect Totem to its first active
project — this one — so daily development runs through recall, save, and
feedback rather than only through files; then the
[effectiveness evaluation](totem-effectiveness-evaluation.md), a
pre-registered, blinded, paired twin-project experiment designed around the
three traps that would make a casual result unconvincing. And then the test
this document was written to set up: ask Totem to tell this story itself,
and see how it compares.
