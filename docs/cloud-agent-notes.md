You are implementing **Totem** — a durable, auditable memory and shared-context system for AI agents: a typed context layer beneath every harness (Claude Code, Cursor, cloud agents) that holds six categories of scoped memory, mirrors the ARRIVE landscape of enrolled repos, and meters the value of what it remembers. It is a Rust workspace (Axum gateway serving MCP + REST, SurrealDB store, ARRIVE sync service, AI curators, Dioxus console) governed by ARRIVE.

The unit of work is one PHASE of the implementation plan (`arrive/implementation-plan.yaml`), implemented one advance at a time on a single shared branch. Each advance lands as its own commit series. At most one phase branch is ever in flight, and the next phase does not start until the current phase's PR is merged. A single run implements as many consecutive advances of the current phase as are designated for its model, then stops.

## Step 1 — Merge/handoff gate (do this FIRST, before anything else)

This routine fires hourly, but the intended pace is one phase per MERGE, not one per hour. Most runs should be no-ops.

```
git fetch origin --prune
git ls-remote --heads origin 'refs/heads/advance/*'
git branch -r --merged origin/master | grep 'origin/advance/' || true
```

Decide as follows, using only cheap git commands — do not bootstrap or read docs to make this decision:

**A. No unmerged `advance/*` branch on origin** → fresh start. Continue to Step 2 in FRESH mode.

**B. An unmerged `advance/phase-<NNN>` branch exists** → inspect its plan file without checking it out:

```
git show origin/advance/phase-<NNN>:arrive/implementation-plan.yaml
```

- If every advance item in that phase has `status: done` → the phase is complete and awaiting review. Report exactly, then stop:

  GATED: advance/phase-<NNN> is complete and awaiting merge into master. No work done this run.

- Otherwise, find the FIRST item in that phase (plan order) with `status: planned`. Apply the model gate (Step 4) to it:
  - It matches your model → continue to Step 2 in CONTINUE mode, targeting that branch and that advance.
  - It does not → report exactly, then stop:

    MODEL-GATED: <ADV-ID> on advance/phase-<NNN> is designated for <model>. Waiting for that routine. No work done this run.

**C. An unmerged legacy `advance/ADV-*` branch exists** (single-advance era) → report the old GATED line and stop; it must merge before phase work begins.

GATED and MODEL-GATED are the expected, correct outcome for most runs. A no-op is a success, not a failure. Exiting here costs a few seconds and is the whole point of the gate.

## Step 2 — Bootstrap ARRIVE

```
./scripts/bootstrap-arrive.sh
export PATH="$HOME/.local/bin:$PATH"
arrive --version
```

If bootstrap-arrive.sh exits non-zero, STOP IMMEDIATELY. Do not implement anything. Report the exact error output verbatim. The script fails on an unsupported architecture, musl libc, or an unusable CLI. Include its architecture line in your report.

Implementing without working ARRIVE commands produces ungoverned changes and is worse than doing nothing.

Do NOT run authenticated commands: no `arrive sync`, no `arrive platform *`, no `arrive auth`. Everything you need (doctor, info, status, score, check, plan, template render, draft, log) works offline.

## Step 3 — Read the governance context

Read in this order:
- `CLAUDE.md` — binding project rules
- `arrive/implementation-plan.yaml` — the sequenced plan (6 passes, 23 advances)
- `arrive/agent-rules/advance-writing.md`, `dev-practices.md`, `reviewability.md`, `advance-profiles.md`
- `docs/project-brief.md` and `docs/solution-intent.md` — the source requirements
- `docs/arrive-decomposition-gaps.md` — decomposition rationale, known gaps, and advance sequencing
- `docs/tech-direction/` — executed findings and pinned decisions (SurrealDB constraints TD-001..TD-011; embedding pin EMB-001..EMB-004). These are binding on store/gateway/console advances — an implementation that contradicts a TD/EMB entry is wrong even if its tests pass

## Step 4 — Pick the work

**Model gate (applies everywhere).** Two routines run this protocol on different models; your routine prompt states which model you are. A plan item whose `notes` contain `MODEL: claude-opus-5` is Opus-only; every other item is Sonnet-only.

**FRESH mode.** Run `arrive plan show` and `arrive info`. Select the FIRST phase in plan order that contains any advance with `status: planned`. Every advance in all earlier phases must be `done` — if not, STOP and report the inconsistency instead of improvising. Within the selected phase, work strictly in plan order; each advance additionally requires every id in its `dependencies` to be `done`.

If the first planned advance of the phase is NOT designated for your model, report the MODEL-GATED line from Step 1 and stop. Do not create the branch and do not skip ahead to a later advance that matches your model — plan order is strict, and the branch belongs to whichever routine starts the phase.

**CONTINUE mode.** The branch and target advance were already determined in Step 1. Verify the target advance's dependencies are `done` on that branch; if not, STOP and report why.

If no phase qualifies, STOP and report why (all done, or everything blocked). Do not invent work. If the plan file is unexpectedly missing, STOP and report that instead of improvising an order.

## Step 5 — Implement, one advance at a time

```
FRESH:    git checkout -b advance/phase-<NNN>
CONTINUE: git checkout advance/phase-<NNN>     # tracks origin; do NOT rebase or force-push it
```

Then loop: implement the current advance completely (this step), record it (Step 6), validate (Step 7), and push (Step 8) — BEFORE touching the next advance. After each advance, look at the next planned advance in the phase:

- It is designated for your model → continue the loop in this same run.
- It is designated for the other model → push, then stop and report a HANDOFF (see Final report). The other routine picks the branch up on its next firing.
- No planned advances remain in the phase → the phase is complete; open the phase PR (Step 8).

The advance file at `arrive/systems/058-totem-core/advances/<ADV-ID>.md` is the specification. Its Objective, Behavioral Change/Outcome, and Planned Implementation Tasks describe what to build. Follow them.

Mandatory practice — commit in this order, as separate commits, prefixing each subject with the advance id (e.g. `test: [ADV-STORE-001] ...`) so the phase branch stays reviewable commit-by-commit:
1. `tidy:` preparatory refactoring only, no behavior change (skip if nothing to tidy)
2. `test:` tests written FIRST, and they must fail for the right reason before you implement
3. `feat:`/`fix:` the minimal implementation that makes them pass

Never interleave commits of two advances. If an advance turns out to be unimplementable mid-phase, record it honestly (Step 6, `in_progress` with the reason), push, and stop — do not skip over it to a later advance.

SurrealDB in this environment:
- Tests MUST use the embedded in-memory engine (`kv-mem` feature): each test constructs its own instance and seeds its own fixtures. Never write a test that assumes a running `surreal` server, a localhost port, or Docker — none exist in this sandbox, and such tests hang or fail here while passing locally.
- Keep the on-disk engine (RocksDB) behind an optional cargo feature so the default test build stays lean; ephemeral runners pay full compile cost on a cold cache.
- Pin the `surrealdb` crate to an exact version when first adding it, so hourly runs are not broken by an upstream release mid-project.

Honesty rules that matter more than speed:
- Only claim `tdd:red-green` if you genuinely wrote failing tests first.
- Check the advance's `mode` and `work_products` frontmatter. If `mode` is `investigation`, `evaluation`, or `enablement`, or the work products are not `production_code` (e.g. test_data, performance_harness, test_automation), do NOT claim TDD. Follow the profile's own practice list in `arrive/agent-rules/advance-profiles.md`.
- Never claim `ci:passed` for a pipeline that has not run. If CI is configured for `advance/**` pushes, you may be able to observe a real pipeline result after pushing — if you can read it, record it honestly; if you cannot, say so.
- If a tool you needed was unavailable, say so explicitly rather than omitting it.
- If you cannot satisfy something the advance requires, say so in the advance body rather than silently narrowing scope.

## Security-critical advances (scope isolation and auth)

Totem's highest-severity failure class, per the project brief, is leaking private context across scopes. The advances that build or guard that boundary — ADV-STORE-001 (scope isolation at the store layer), ADV-CORE-003 (scope promotion), ADV-GATEWAY-003 (token auth for cloud agents), ADV-CLI-001 (credential issuance), ADV-GATEWAY-006 (security evaluation) — deserve extra care. A scope-leak defect will compile, pass casual review, and quietly betray every enrolled developer.

Specific hazards to actively avoid, not merely keep in mind:

- **Scope filtering above the store.** Filtering results in the gateway or console instead of the store layer turns the isolation invariant into an application courtesy. Every read path — including landscape queries, live-query subscriptions, and curator jobs — must go through store-enforced scope resolution.
- **Episodic mutation.** Episodic records are append-only. Any code path that updates or deletes one — including "harmless" backfills — breaks the audit substrate.
- **Provenance-free writes.** Every write carries provenance. A convenience constructor that defaults provenance defeats auditability.
- **Over-scoped tokens.** Cloud credentials are least-privilege, bound to repo + scope. A token that can enumerate other repos or read an unbound scope is a leak vector even if no data flows today.
- **Unlogged access.** Every read and write appends to the access log. A code path that can touch memory without logging is an audit gap — check error paths and internal/curator calls, not just the happy path.

The component invariants live in `arrive/systems/058-totem-core/components/*.yaml`. All components are stage `incubating`, so the CLI does not enforce them — they are binding design intent enforced only by your honesty. State plainly in the advance any invariant you established only partially.

Write the tests for these invariants before the implementation and make sure they fail for the right reason. A test that passes against a deliberately broken implementation is worse than no test.

If you find a factual error in the advance itself (a claim about enforcement, a wrong reference, an unachievable outcome), correct it in the advance and say so in your report. The advances were authored ahead of implementation and can be wrong.

## Step 6 — Update the advance file (after EACH advance)

In `arrive/systems/058-totem-core/advances/<ADV-ID>.md` frontmatter:
- `status: complete` (only if genuinely done; otherwise leave `in_progress` and say why)
- `implementation_completed_at:` current UTC RFC3339 timestamp
- `evidence:` only what you actually produced
- `reviewability_score:` from `arrive score`

In the body:
- Replace the `## Changes Made` placeholder with real dated entries: `### <date> - <prefix>: <summary>` followed by `- <file>: <what changed>`
- FULLY REPLACE `## Check for Understanding` with fresh questions grounded in the files you actually changed. Stale CFU is not allowed.

Also set the matching item's `status: done` in `arrive/implementation-plan.yaml`, then run `arrive plan check`. This per-advance plan update is what lets the other routine (and the gate in Step 1) see exactly how far the phase has progressed — never batch it to the end of the run.

Commit these governance updates as part of the advance's own commit series (e.g. `docs: [ADV-STORE-001] complete advance record`), before starting the next advance.

## Step 7 — Validate before pushing (after EACH advance)

All must pass:
```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
arrive doctor artifacts
arrive plan check
arrive check --strict
arrive score
```

Record the `arrive score` for the advance you just finished — per-advance scores are what get reported, since the phase PR is expected to exceed the Green budget in aggregate. That aggregate overrun is accepted BY DESIGN for phase PRs: review happens commit-by-commit, advance-by-advance. If a SINGLE advance scores Red (>60), either split the work or add a `## Reviewability` section to that advance justifying why the change is atomic. Say which you chose.

If any check fails and you cannot fix it, push what you have on the branch and report the failure plainly. Do not disable a test, relax a lint, or weaken an assertion to get green — that defeats the point of the gates.

## Step 8 — Push after each advance; PR when the phase completes

```
git push -u origin advance/phase-<NNN>
```

Push at the end of every advance, even mid-phase — the pushed branch and updated plan file ARE the handoff and crash-recovery state. Never push directly to `master`. Never force-push (a rewritten phase branch breaks the other routine's CONTINUE mode).

Open the PR only when the LAST advance of the phase is complete — whichever routine finishes the phase opens it:

```
gh pr create --title "phase-<NNN>: <short phase summary>" --body "..."
```

If `gh` is not authenticated, report that the PR needs opening manually and give the compare URL:
https://github.com/srswart/totem/pull/new/advance/phase-<NNN>

PR body must include: the phase id; each advance id with its objective, what changed, its evidence, and its per-advance reviewability score; a note that this is a phase PR reviewed commit-by-commit; and anything you could not satisfy. If handing off mid-phase, do NOT open a PR — report the compare URL and the HANDOFF line instead.

## Step 9 — Preserve the journey report (mandatory when not gated)

The final report is our record of the journey — it must survive the session. After the last push of the run (phase PR opened, handoff, or partial), write the full final report (the same content as the "Final report" section below) to:

```
docs/agent-journey/<UTC-date>T<UTC-time>Z-phase-<NNN>.md      e.g. docs/agent-journey/2026-08-12T140503Z-phase-004.md
```

Start the file with this frontmatter so the reports are analyzable in aggregate later:

```yaml
---
run_at: "<UTC RFC3339>"
phase: "phase-<NNN>"
advances: ["<ADV-ID>", ...]          # the advances THIS RUN worked on
outcome: phase-complete | handoff | partial | failed | blocked
reviewability_scores: {"<ADV-ID>": <n>, ...}
checks_passed: [...]
checks_failed: [...]
branch: "advance/phase-<NNN>"
pr: "<PR or compare URL>"
---
```

Then commit it on the phase branch with prefix `docs:` and push again — the phase PR (once open) updates automatically, so the reports merge into master together with the work they describe:

```
git add docs/agent-journey/
git commit -m "docs: agent journey report for phase-<NNN>"
git push
```

This applies to handoff, partial, and failed runs too — those reports are the most valuable to analyze. If the run failed before a branch existed (e.g. bootstrap failure at Step 2), create the report on a branch named `journey/<UTC-date>-bootstrap-failure`, push it, and note it in your report. Only fully gated Step-1 runs skip this step: they produce no branch changes and only the GATED / MODEL-GATED line.

## Final report

If gated at Step 1, report only the GATED or MODEL-GATED line — nothing else is needed.

Otherwise end with:
1. The architecture line printed by bootstrap-arrive.sh
2. The phase, mode (FRESH or CONTINUE), and which advances THIS RUN implemented, each marked complete or partial
3. The run outcome: phase-complete, HANDOFF (naming the next advance and its model), or partial/failed
4. Check results per advance — passes AND failures
5. Per-advance reviewability scores, and for any Red advance whether you split or documented
6. Any correction you made to an advance itself
7. Any invariant you established only partially, stated plainly
8. The branch name, and the PR link (phase complete) or compare URL (handoff/partial)
9. Confirmation that the journey report was committed (its path), per Step 9

Be accurate about failures. A truthful report of a partial run is far more useful than an optimistic one.
