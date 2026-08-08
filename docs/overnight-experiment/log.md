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

## 2026-08-06 — `worked` — The workstation/cloud boundary became a first-class routing designation

Advances that need what the sandbox cannot provide (browser, Docker, blocked
downloads, judgment calls) are now marked `WORKSTATION` in plan notes (PR #26):
cloud runs skip them when selecting work, dependencies stay binding, and
`WORKSTATION-GATED` is the no-op line when nothing cloud-eligible remains. The
day/night split is now expressed in the same vocabulary as the Opus/Sonnet
model gate — who, and where.

## 2026-08-06 — `failure-mode` — Stale plan notes caused a false status flip on a done advance

ADV-STORE-006's plan entry carried a leftover "BLOCKED on environment... move
to planned only when running it on a capable host" note from before its
execution, while its status correctly said `done`. Reading only the plan, both
the assistant and the human concluded the advance had never run and "restored"
it to `planned` (PR #26) — but the advance *file* showed it fully executed the
day before (PR #4, TD-009..TD-011, residual retired). Corrected same-day.

**Implications:** (1) the advance file is the authoritative record; never
change a plan status without reading it. (2) Plan `notes` rot — a status
transition should rewrite the note, not append to history. (3) `arrive plan
check` validated both the wrong state and the right one; it does not
cross-check plan status against advance frontmatter status — a tooling gap
worth an upstream request. (4) For the overnight cycle: agents made neither
error — the run that marked it done rewrote nothing misleading in its own
records; the divergence was introduced and later fixed by day-side humans.
The audit trail (advance file + PR history) is what caught it.

## 2026-08-06 — `worked` — Demo milestone: every shipped layer functions end-to-end

First live run on a workstation (2026-08-06): gateway up, CLI-enrolled this
repo (1 system / 7 components / 26 advances into the landscape), memory
save/recall over REST with scope isolation verified at the API and in the UI
(an actor:alice memory invisible to shawn), and the Dioxus console rendering
in a real browser for the first time — closing ADV-CONSOLE-001's
"unverified in browser" residual and confirming by hand the Refresh-only
behavior ADV-CONSOLE-003 exists to fix. Everything is per-process in-memory;
the demo promoted the §9 topology decision (and reserved ADV-INFRA-001) from
"documented someday" to the blocker between demo and daily use.

## 2026-08-07 — `failure-mode` — A stranded PR amendment un-decided a decision; the agent faithfully executed stale governance

The ADV-CONSOLE-004 WORKSTATION conversion was pushed as an amendment to an
open PR (#36) that the human had already merged — the commit landed on a
dead branch and silently never reached master or the phase-008 plan copy. A
manually-fired run then read its (stale) governance, correctly selected
CONSOLE-004 as cloud-eligible, and claimed it — ten minutes from a
vanilla-CSS redesign nobody wanted. Caught by the human noticing an
unexpected active session; stopped with only a claim pushed; the amendment
was cherry-picked onto the phase branch same-day.

**Implications:** (1) The agent was blameless — reviewing the *system's*
records, not the agent's behavior, found the bug; the second time
(cf. ADV-STORE-006) the failure lived in day-side record handling. (2) New
habit: amendments to an open PR are announced before the merge button is
plausible, or go in a fresh PR — the merge race has now bitten twice
(PR #11, PR #36). (3) A gate that read only merged state would have been
immune; the claim-first design still contained the blast radius to one
stale claim branch.

## 2026-08-08 — `failure-mode` — Four advances, two days, one blind spot: we test insides, not edges

Four defects in two days reached a deployed system through a fully green
suite, and all four sit in the same place — the boundary where our code meets
something we do not run in CI.

| Advance | Defect | Why the suite missed it |
|---|---|---|
| GATEWAY-013 | `totem_save`'s published JSON Schema declared no type for `author`; claude.ai sent a string | Tests called the handler in-process with typed arguments; nobody read the *published* schema |
| CLI-002 | The CLI could not reach an HTTPS gateway at all (no TLS feature) | Every test used plain-HTTP loopback |
| GATEWAY-015 | `json_response = true` had never applied to a real client | No test drove the transport; the comment asserting otherwise was false from the day it was written |
| GATEWAY-010 | AuthKit's token endpoint is unreachable from a browser; the console flashed a dashboard on every load | No browser in CI, and no way to add one |

**The pattern:** an in-process test verifies that our function does what we
meant. It cannot verify a *contract with something external* — a published
schema, a TLS handshake, a wire format, a CORS response header, a rendered
frame. Three of the four had a comment or a record asserting the correct
behavior, written by whoever wrote the code, and the assertion was simply
wrong. A green suite plus a confident comment is not evidence about an edge.

**Implications for the overnight cycle:**

1. **This is a boundary between night work and day work, and a sharp one.**
   Everything above was found by a human at a browser or a real client. An
   autonomous run can build the console, prove it compiles, prove the routes
   do not shadow, and deploy it — and still cannot learn that the sign-in
   fails. Advances whose evidence is *external* should be routed as work the
   night shift prepares and the morning verifies, rather than work the night
   shift closes.
2. **Evidence keys already encode this** and should be used deliberately.
   `login:executed`, `connection:executed` and `restore:verified` are all
   claims no sandbox can earn. An advance whose evidence list contains one is
   structurally a day/night handoff, and the plan could say so the way it
   says `WORKSTATION`.
3. **The cheap fix is not "more tests" but "one test at the real boundary."**
   GATEWAY-013 got three published-schema tests, GATEWAY-015 three
   client-shaped rmcp-over-HTTP tests, CLI-002 a real TLS path. Each was
   small. None existed because nothing prompted them; the advance template
   asks what to test, not *at which boundary*.
4. **The console's edge stays uncovered.** There is no browser in CI and this
   trial is not buying one. That is an accepted gap, not an oversight —
   worth restating whenever console work is scheduled for a cloud run.

## 2026-08-08 — `failure-mode` — An evaluation predicted the defect, sat unrun for three days, and would have passed anyway

ADV-CORE-005 ("Quality evaluation: recall relevance + ranking behavior") was
authored on 2026-08-05. Its Quality Risks section named the hazard before
anyone had seen it: *"ranking regressions from the value/currency
multipliers."*

On 2026-08-08 that risk was confirmed on the deployed instance — four
queries, two of them verbatim copies of a stored record's own body, all
returning the same seven records in the same order. Recall was ignoring the
query entirely. In between, six advances shipped past a planned evaluation
that had already written down what would go wrong.

**The obvious lesson is wrong, and the correction is the actual finding.**

"Run the evaluation earlier" would not have caught this. `eval_quality`
asserts `precision_at_1 == 1.0` and **passes today**, against the broken
system, because `crates/totem-store/src/corpus.rs` never sets `value_score`
or `last_used_at` — the words do not appear in the file. Every golden record
carries identical pristine economics, so relevance is the only term that
varies and ranking looks perfect. The defect needs *accumulated usage* to
appear, which is the state every real estate is permanently in and no fixture
had ever been in.

Run on 2026-08-05, ADV-CORE-005 would have reported a confident 1.0 and been
recorded as evidence that recall quality was good. **Early would have been
worse than late** — a green number is harder to revisit than an open box.

**So the finding is two-layered:**

1. **Evaluations placed at the end can only ratify; placed early they can
   steer.** By the time an evaluation runs after six advances, its findings
   are rework. That is a real argument for moving some of them earlier —
   Shawn's point, and worth a standing practice rather than a one-off.
2. **An evaluation is only as good as the state its fixtures model.** Moving
   a vacuous evaluation earlier manufactures false confidence sooner. The
   sequencing question and the fixture-realism question have to be asked
   together, and the second one is the one nobody asks.

**Proposed practice (not yet adopted):** a periodic forward review of planned
advances — not "what is next" but "what have we written down that we are
walking past, and would it actually detect what it claims to?" Two questions
per evaluation advance:

- *What state does its fixture model, and is the system ever in that state?*
- *Can it fail?* An evaluation that has never produced a failing number is
  unproven as an instrument.

**For the overnight cycle specifically:** evaluation advances are unusually
good night work — read-only, bounded, no deployment, no judgement calls about
what to build. But this entry is the caveat. A night agent running a vacuous
evaluation produces a green number and a confident record, and green numbers
are exactly what nobody re-examines in the morning. Before an evaluation is
routed to a cloud run, somebody has to have seen it fail at least once.

**Related:** the fixture gap is now a task in ADV-CORE-008; ADV-CORE-005
carries the confirmed defect as a pre-registered finding and a warning that
any number it produces before those fixtures exist is unmeasured rather than
passing. No new harness or evaluation advance was authored — ADV-GATEWAY-008
(scorer) and ADV-STORE-005 (corpus) already existed and were done, which is
itself a small lesson about reaching for a new advance when an evaluation
comes back green.
