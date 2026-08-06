# Overnight Development Experiment — Observation Log

Totem is, beyond its own product goals, an experiment in an overnight
development cycle: scheduled cloud agents execute pre-specified work at night
under ARRIVE governance; staff review, decide, and write runway during the
day. This log records observations from that standpoint — not what was built,
but what the *process* revealed. The agent journey reports in
`docs/agent-journey/` are the raw telemetry; this file is the curated
findings.

Entry format: date, category, observation, implication. Categories:
`worked` (behavior to keep), `failure-mode` (something broke, and how),
`boundary` (evidence about what agents can/can't be trusted with),
`open-question`.

Two standing questions this experiment exists to answer:

1. What type of work makes sense for agents to do autonomously?
2. What should happen during the day, with staff?

---

## 2026-08-05 — `boundary` — Agents execute decided work well; run-time judgment drifts

The first four implementation advances (STORE-001/002, GATEWAY-001/002) all
arrived complete with green validations. Every one also scored Red on
reviewability (101, 105, 70, 99) and every one chose "documented, not split" —
the third citing the first as precedent. Each justification read as
individually reasonable; collectively, "split unless documented" had
operationally inverted into "document by default" within four runs.

**Implication:** agents follow specs excellently and rationalize judgment
calls persuasively. Anything that is a *policy* (what deserves splitting, what
scope is acceptable) needs either a hard rule or a human decision point —
run-time agent judgment calibrates to whatever the humans last accepted.

## 2026-08-05 — `worked` — Gate-by-pushed-state makes unattended firing safe

Hourly firings while a sub-PR sat unreviewed correctly produced GATED /
MODEL-GATED no-ops costing seconds. The system idles safely when the human is
away; nobody has to remember to pause the schedule.

## 2026-08-06 — `worked` — Merge-triggered handoffs chain without human choreography

Overnight (19:00–05:00 UTC), each human merge released exactly the next unit:
sub-PR merge → next advance; phase-complete → agent opened the phase PR
itself; phase PR merge → next phase started FRESH. The human only ever
performed reviews and merges; all sequencing decisions were derived by agents
from pushed state (plan file on the phase branch).

## 2026-08-06 — `failure-mode` — Duplicate-selection collision (ADV-CLI-001)

Two Sonnet sessions passed the gate within minutes of each other (a cron
firing, plus the previous run looping back to Step 1 after its sub-PR merged
mid-session). Both implemented ADV-CLI-001 independently for ~30 minutes. The
gate reads only *pushed* state, and a run's first push came ~30 minutes after
its gate check — any two gate checks inside that window duplicate work.

The losing run's conduct was the notable part: push rejected → refused to
force-push → preserved its complete implementation on a `journey/*-collision`
ref → reported `blocked` honestly. Cost: one duplicated run of compute. The
collision also accidentally produced two independent implementations of a
security-critical advance, which made the surviving sub-PR's review *richer*
(scope divergence between the two was the key review question).

**Fixes adopted** (PR #23): claim-first push (empty commit before
implementing, closing the window to seconds); one-advance-per-run as an
explicit hard stop; collision etiquette codified from the losing run's
improvisation.

## 2026-08-06 — `failure-mode` — Honest partial completion deadlocked the gate (ADV-CONSOLE-001)

The console run shipped working read-only scope but declined to claim
completion (spec promised live auto-update; sandbox had no browser to verify
in), recording `in_progress` with reasons — exactly what the honesty rules
demand. The gate had no defined behavior for `in_progress`, so every
subsequent hourly run deadlocked on a GATED (plan inconsistency) line until a
human decided: accept corrected scope and defer the residual to a new planned
advance (ADV-CONSOLE-003), or re-scope.

**Implication — and the shape of a good failure:** loud, stationary,
reversible. The honesty rules worked, the gate failed *safe* (deadlock, not a
bad merge), and the blocked run pushed a notification naming the exact human
decision needed. Scope decisions are inherently day-side; the night side's job
is to stop and say so clearly. Case-C `in_progress` handling is now defined
(PR #23).

## 2026-08-06 — `boundary` — Some work is inherently day-side / workstation-side

Accumulated examples: browser verification (`dx serve` — no browser in the
cloud sandbox), ADV-STORE-006's server-parity investigation (egress proxy
blocks the SurrealDB binary download), review of security-critical advances,
reviewability policy calls, and scope decisions on partial completions. The
plan's `notes` field absorbed these constraints case-by-case; a first-class
"workstation-only" designation may be warranted if the list grows.

## 2026-08-06 — `worked` — Honesty rules produce trustworthy telemetry

Observed: partial TDD claimed as partial (ARRIVE-SYNC-001 disclosed which
modules were red-first and which weren't); `in_progress` chosen over a false
`complete` (CONSOLE-001); a collision reported as `blocked` with the work
preserved rather than silently discarded (CLI-001). The journey reports can
be believed, which is what makes aggregate analysis of this experiment
possible at all.

## 2026-08-06 — `open-question` — Throughput is review-bounded; what earns auto-merge?

With the strict serial gate, an untouched night yields exactly one unmerged
sub-PR. The candidate evolution is risk-tiered gating: Green-scored,
non-security, non-production work products (docs, test hardening,
investigations) auto-merge on green CI at night; Red scores, security-critical
advances, and scope-boundary work always wait for a human. ARRIVE's
reviewability score + advance profiles + the security-critical list provide
the tiers for free. Deliberately deferred until enough journey reports exist
to justify each tier with data ("which advance types consistently arrive
complete, green, and boring").

## 2026-08-06 — `open-question` — Night-shift supervisor agent

Under consideration: an AI engineer on the night shift — a supervisory agent
that watches runs and intervenes when things go off track, so mornings start
with prepared decisions instead of raw incidents. See the day's incidents for
its candidate job description: explain a collision, stage the fix options for
a deadlock, flag a suspicious pattern (four documented-Reds). Open questions:
authority boundary (it must not absorb the human's review/merge/scope role),
cost of a standing session vs. an hourly triage pass, and whether its
interventions should themselves be logged here.
