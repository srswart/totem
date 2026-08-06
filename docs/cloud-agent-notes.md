You are implementing **Totem** — a durable, auditable memory and shared-context system for AI agents: a typed context layer beneath every harness (Claude Code, Cursor, cloud agents) that holds six categories of scoped memory, mirrors the ARRIVE landscape of enrolled repos, and meters the value of what it remembers. It is a Rust workspace (Axum gateway serving MCP + REST, SurrealDB store, ARRIVE sync service, AI curators, Dioxus console) governed by ARRIVE.

The plan (`arrive/implementation-plan.yaml`) is implemented one PHASE at a time, and each phase one ADVANCE at a time:

- Each phase gets an integration branch `advance/phase-<NNN>` and, at the end, ONE PR into master.
- Each advance is implemented on its own sub-branch `advance/sub/phase-<NNN>/<ADV-ID>` and delivered as a **sub-PR into the phase branch**, where it is reviewed and merged individually.
- A run implements at most ONE advance. The next advance does not start until the previous advance's sub-PR is merged, and the next phase does not start until the phase PR is merged into master.

The three branch tiers matter — never confuse them: `advance/phase-<NNN>` (phase integration), `advance/sub/phase-<NNN>/<ADV-ID>` (one advance, one sub-PR), `advance/ADV-*` (legacy single-advance era; if one exists unmerged, it simply gates).

## Step 1 — Gate (do this FIRST, before anything else)

This routine fires hourly, but the intended pace is one advance per SUB-PR MERGE, not one per hour. Most runs should be no-ops.

```
git fetch origin --prune
git ls-remote --heads origin 'refs/heads/advance/*'
```

Classify what exists, using only cheap git commands — do not bootstrap or read docs to make this decision:

**A. A legacy `advance/ADV-*` branch is unmerged into `origin/master`** → report the old line and stop:

  GATED: advance/<BRANCH> is pushed but not merged into master. Waiting for review. No work done this run.

**B. A sub-branch `advance/sub/phase-<NNN>/<ADV-ID>` exists and is NOT merged into `origin/advance/phase-<NNN>`** (check with `git branch -r --merged origin/advance/phase-<NNN>` — a merged-but-undeleted sub-branch does not gate) → an advance is in review. Report exactly, then stop:

  GATED: advance/sub/phase-<NNN>/<ADV-ID> is awaiting sub-PR merge into advance/phase-<NNN>. No work done this run.

**C. A phase branch `advance/phase-<NNN>` exists, unmerged into master, with no unmerged sub-branches** → inspect its plan file without checking it out:

```
git show origin/advance/phase-<NNN>:arrive/implementation-plan.yaml
```

- **Every advance item in that phase is `done`** → the phase is complete. If no PR from `advance/phase-<NNN>` into master exists yet, open it now (`gh pr create --base master`, body per Step 8; if `gh` is unavailable report the compare URL) — this is the one piece of work permitted in a gated run. Then report, then stop:

  GATED: advance/phase-<NNN> is complete and awaiting merge into master. No implementation done this run.

- **Any item in the phase has `status: in_progress`** → a previous run deliberately recorded partial completion. This is a human decision point, not agent work: a human must either accept the corrected scope (advance → complete, plan item → done, residuals deferred to a new planned advance) or re-scope the item. Do not implement past it, do not mark it done yourself, do not pick a later planned item. Report exactly, then stop:

  GATED (plan inconsistency): advance/phase-<NNN> has <ADV-ID> at status: in_progress — awaiting a human scope decision. No work done this run.

- **Otherwise** → find the FIRST item in the phase (plan order) with `status: planned`, skipping any whose `notes` contain `WORKSTATION` (see the workstation gate in Step 4; if only WORKSTATION items remain, report the WORKSTATION-GATED line and stop). Apply the model gate (Step 4). Match → continue to Step 2 in CONTINUE mode targeting that advance. No match → report exactly, then stop:

  MODEL-GATED: <ADV-ID> on advance/phase-<NNN> is designated for <model>. Waiting for that routine. No work done this run.

**D. No unmerged `advance/*` branches at all** → fresh start. Continue to Step 2 in FRESH mode.

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

## Step 4 — Pick the advance

**Model gate (applies everywhere).** Two routines run this protocol on different models; your routine prompt states which model you are. A plan item whose `notes` contain `MODEL: claude-opus-5` is Opus-only; every other item is Sonnet-only.

**Workstation gate (applies everywhere).** A plan item whose `notes` contain `WORKSTATION` is never cloud-eligible: it needs something this sandbox cannot provide (a browser, Docker, a blocked download, a human judgment call). When selecting "the first planned item", skip WORKSTATION items as if they were done — but dependencies stay binding: an item that depends on an unfinished WORKSTATION item is not eligible either. WORKSTATION items are implemented from a workstation session (following the same sub-branch/sub-PR discipline when their phase is active; via a standalone PR to master when their phase has already merged). If the only reachable planned work is WORKSTATION-designated or blocked behind one, report exactly, then stop:

  WORKSTATION-GATED: <ADV-ID> requires local execution. Waiting for a workstation session. No work done this run.

**FRESH mode.** Run `arrive plan show` and `arrive info`. Select the FIRST phase in plan order that contains any advance with `status: planned`, ignoring WORKSTATION-designated items when scanning (a lingering planned WORKSTATION item in an earlier phase is expected, not an inconsistency). Every non-WORKSTATION advance in all earlier phases must be `done` — if not, STOP and report the inconsistency instead of improvising. The target is the first planned, non-WORKSTATION advance of that phase; it additionally requires every id in its `dependencies` to be `done`. If it is not designated for your model, report the MODEL-GATED line and stop — do not create any branch, and do not skip ahead to a later advance that matches your model. Plan order is strict among cloud-eligible items.

**CONTINUE mode.** The phase and target advance were already determined in Step 1. Verify the target advance's dependencies are `done` on the phase branch; if not, STOP and report why.

Either way: implement exactly ONE advance this run. This is a hard invariant, not a target — after your sub-PR is opened (or your outcome is otherwise concluded), the run ENDS. Never loop back to Step 1 for another advance, even if the gate has since opened (for example, your sub-PR was reviewed and merged while you were still writing reports). The next advance belongs to the next run.

If no phase qualifies, STOP and report why (all done, or everything blocked). Do not invent work. If the plan file is unexpectedly missing, STOP and report that instead of improvising an order.

## Step 5 — Implement the advance on a sub-branch

```
FRESH:    git checkout -b advance/phase-<NNN> origin/master
          git push -u origin advance/phase-<NNN>          # create the integration target first
CONTINUE: git checkout -b advance/phase-<NNN> origin/advance/phase-<NNN>

Then:     git checkout -b advance/sub/phase-<NNN>/<ADV-ID>
```

**Claim first.** Immediately after creating the sub-branch — before reading the advance spec, before any implementation — push it with an empty claim commit:

```
git commit --allow-empty -m "chore: [<ADV-ID>] claim"
git push -u origin advance/sub/phase-<NNN>/<ADV-ID>
```

The Step 1 gate blocks on unmerged sub-branches, so the claim shrinks the window in which two concurrent runs can select the same advance from the length of an implementation (30+ minutes) to seconds. A claim branch whose last push is hours old with no sub-PR is probably a dead run — still GATE on it, note the suspicion in your report, and leave cleanup to a human; no run ever deletes or overwrites another's branch.

ALL commits for the advance go on the sub-branch. Never commit directly to the phase branch, never push to master, and never force-push any branch that has been pushed — a rewritten branch breaks its open PR and the other routine.

**On push collision.** If a push to the sub-branch is rejected because it already exists on origin with someone else's commits, two runs selected the same advance inside the claim window. Do NOT force-push, do NOT open a competing sub-PR, do NOT try to merge the two implementations. The first push owns the advance. Preserve your full commit series on a fresh ref `journey/<UTC-timestamp>-<ADV-ID>-collision`, push that, and report the collision plainly (outcome: blocked) — the preserved branch is comparison material for whoever reviews the surviving sub-PR.

The advance file at `arrive/systems/058-totem-core/advances/<ADV-ID>.md` is the specification. Its Objective, Behavioral Change/Outcome, and Planned Implementation Tasks describe what to build. Follow them.

Mandatory practice — commit in this order, as separate commits, prefixing each subject with the advance id (e.g. `test: [ADV-STORE-001] ...`) so the phase branch's history stays legible after sub-PRs merge:
1. `tidy:` preparatory refactoring only, no behavior change (skip if nothing to tidy)
2. `test:` tests written FIRST, and they must fail for the right reason before you implement
3. `feat:`/`fix:` the minimal implementation that makes them pass

If the advance turns out to be unimplementable, record it honestly (Step 6, `in_progress` with the reason), push the sub-branch, open the sub-PR anyway marked as partial, and report plainly.

SurrealDB in this environment:
- Tests MUST use the embedded in-memory engine (`kv-mem` feature): each test constructs its own instance and seeds its own fixtures. Never write a test that assumes a running `surreal` server, a localhost port, or Docker — none exist in this sandbox, and such tests hang or fail here while passing locally.
- Keep the on-disk engine (RocksDB) behind an optional cargo feature so the default test build stays lean; ephemeral runners pay full compile cost on a cold cache.
- Pin the `surrealdb` crate to an exact version when first adding it, so hourly runs are not broken by an upstream release mid-project.

Honesty rules that matter more than speed:
- Only claim `tdd:red-green` if you genuinely wrote failing tests first.
- Check the advance's `mode` and `work_products` frontmatter. If `mode` is `investigation`, `evaluation`, or `enablement`, or the work products are not `production_code` (e.g. test_data, performance_harness, test_automation), do NOT claim TDD. Follow the profile's own practice list in `arrive/agent-rules/advance-profiles.md`.
- Never claim `ci:passed` for a pipeline that has not run. CI runs on `advance/**` pushes, so your own push produces a real pipeline result — if you can read it, record it honestly; if you cannot, say so.
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

## Step 6 — Update the advance file

In `arrive/systems/058-totem-core/advances/<ADV-ID>.md` frontmatter:
- `status: complete` (only if genuinely done; otherwise leave `in_progress` and say why)
- `implementation_completed_at:` current UTC RFC3339 timestamp
- `evidence:` only what you actually produced
- `reviewability_score:` from `arrive score`

In the body:
- Replace the `## Changes Made` placeholder with real dated entries: `### <date> - <prefix>: <summary>` followed by `- <file>: <what changed>`
- FULLY REPLACE `## Check for Understanding` with fresh questions grounded in the files you actually changed. Stale CFU is not allowed.

Also set the matching item's `status: done` in `arrive/implementation-plan.yaml`, then run `arrive plan check`. The plan file's per-advance status — as merged into the PHASE branch by your sub-PR — is what the Step 1 gate reads to see how far the phase has progressed, so this update must travel inside the sub-PR, never be batched or left for later.

Commit these governance updates on the sub-branch as part of the advance's commit series (e.g. `docs: [ADV-STORE-001] complete advance record`).

## Step 7 — Validate before pushing

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

The sub-PR is the unit of review, so the normal reviewability budget applies to it directly: if `arrive score` is Red (>60), either split the work or add a `## Reviewability` section to the advance justifying why the change is atomic. Say which you chose.

If any check fails and you cannot fix it, push what you have on the sub-branch and report the failure plainly. Do not disable a test, relax a lint, or weaken an assertion to get green — that defeats the point of the gates.

## Step 8 — Push and open the sub-PR

```
git push -u origin advance/sub/phase-<NNN>/<ADV-ID>
gh pr create --base advance/phase-<NNN> --title "<ADV-ID>: <title>" --body "..."
```

The sub-PR's base is the PHASE branch, not master — double-check before creating; a sub-PR accidentally based on master bypasses the phase integration entirely. If `gh` is not authenticated, report that the PR needs opening manually and give the compare URL:
https://github.com/srswart/totem/compare/advance/phase-<NNN>...advance/sub/phase-<NNN>/<ADV-ID>

Sub-PR body must include: the advance ID and objective, what changed, the evidence you actually captured, the reviewability score, anything you could not satisfy, and this reviewer note verbatim: "Merge with 'Create a merge commit' or 'Rebase and merge' — do NOT squash, the tidy/test/feat commit series is deliberate."

**Phase PR** (opened from Step 1-C when the last sub-PR has merged): base master, title `phase-<NNN>: <short phase summary>`, body listing each advance with its sub-PR link and reviewability score, plus a note that every advance was already reviewed via its sub-PR so this PR is the integration formality.

## Step 9 — Preserve the journey report (mandatory when not gated)

The final report is our record of the journey — it must survive the session. After the sub-PR is opened (or its compare URL determined), write the full final report (the same content as the "Final report" section below) to:

```
docs/agent-journey/<UTC-date>T<UTC-time>Z-<ADV-ID>.md      e.g. docs/agent-journey/2026-08-12T140503Z-ADV-STORE-001.md
```

Start the file with this frontmatter so the reports are analyzable in aggregate later:

```yaml
---
run_at: "<UTC RFC3339>"
phase: "phase-<NNN>"
advance: "<ADV-ID>"
outcome: complete | partial | failed | blocked
reviewability_score: <n>
checks_passed: [...]
checks_failed: [...]
branch: "advance/sub/phase-<NNN>/<ADV-ID>"
pr: "<sub-PR or compare URL>"
---
```

Then commit it on the sub-branch with prefix `docs:` and push again — the open sub-PR updates automatically, so the report merges into the phase branch (and eventually master) together with the work it describes:

```
git add docs/agent-journey/
git commit -m "docs: agent journey report for <ADV-ID>"
git push
```

This applies to partial and failed runs too — those reports are the most valuable to analyze. If the run failed before a branch existed (e.g. bootstrap failure at Step 2), create the report on a branch named `journey/<UTC-date>-bootstrap-failure`, push it, and note it in your report. Only fully gated Step-1 runs skip this step: they produce no implementation branch and only the GATED / MODEL-GATED line (plus, in case 1-C, the phase PR).

## Final report

If gated at Step 1, report only the GATED or MODEL-GATED line — plus the phase PR link if this run opened it (case 1-C). Nothing else is needed.

Otherwise end with:
1. The architecture line printed by bootstrap-arrive.sh
2. The phase, mode (FRESH or CONTINUE), and which advance you implemented — complete or partial
3. Check results — passes AND failures
4. The reviewability score, and whether you split or documented
5. Any correction you made to the advance itself
6. Any invariant you established only partially, stated plainly
7. The sub-branch name and sub-PR link (or compare URL if gh was unavailable)
8. Confirmation that the journey report was committed (its path), per Step 9

Be accurate about failures. A truthful report of a partial run is far more useful than an optimistic one.
