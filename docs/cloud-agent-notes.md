You are implementing **Totem** — a durable, auditable memory and shared-context system for AI agents: a typed context layer beneath every harness (Claude Code, Cursor, cloud agents) that holds six categories of scoped memory, mirrors the ARRIVE landscape of enrolled repos, and meters the value of what it remembers. It is a Rust workspace (Axum gateway serving MCP + REST, SurrealDB store, ARRIVE sync service, AI curators, Dioxus console) governed by ARRIVE. Implement at most ONE advance this run, then stop.

## Step 1 — Merge gate (do this FIRST, before anything else)

This routine fires hourly, but the intended pace is one advance per MERGE, not one per hour. Most runs should be no-ops.

```
git fetch origin --prune
git ls-remote --heads origin 'refs/heads/advance/*'
git branch -r --merged origin/master | grep 'origin/advance/' || true
```

If any `advance/*` branch exists on origin that is NOT merged into `origin/master`, STOP IMMEDIATELY. Do no other work — do not bootstrap, do not read docs, do not implement. Report exactly:

  GATED: advance/<BRANCH> is pushed but not merged into master. Waiting for review. No work done this run.

This is the expected, correct outcome for most runs. A no-op is a success, not a failure. Exiting here costs a few seconds and is the whole point of the gate.

Only if every `advance/*` branch on origin is already merged into `origin/master` (or none exist at all) do you continue to Step 2.

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

## Step 4 — Pick the next advance

Run `arrive plan show` and `arrive info`.

Select the FIRST advance in plan order where BOTH are true:
- its `status` is `planned`
- every id in its `dependencies` has status `done` in the plan

If no advance qualifies, STOP and report why (all done, or everything blocked). Do not invent work. If the plan file is unexpectedly missing, STOP and report that instead of improvising an order.

## Step 5 — Implement it

```
git checkout -b advance/<ADV-ID>
```

The advance file at `arrive/systems/058-totem-core/advances/<ADV-ID>.md` is the specification. Its Objective, Behavioral Change/Outcome, and Planned Implementation Tasks describe what to build. Follow them.

Mandatory practice — commit in this order, as separate commits:
1. `tidy:` preparatory refactoring only, no behavior change (skip if nothing to tidy)
2. `test:` tests written FIRST, and they must fail for the right reason before you implement
3. `feat:`/`fix:` the minimal implementation that makes them pass

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

## Step 6 — Update the advance file

In `arrive/systems/058-totem-core/advances/<ADV-ID>.md` frontmatter:
- `status: complete` (only if genuinely done; otherwise leave `in_progress` and say why)
- `implementation_completed_at:` current UTC RFC3339 timestamp
- `evidence:` only what you actually produced
- `reviewability_score:` from `arrive score`

In the body:
- Replace the `## Changes Made` placeholder with real dated entries: `### <date> - <prefix>: <summary>` followed by `- <file>: <what changed>`
- FULLY REPLACE `## Check for Understanding` with fresh questions grounded in the files you actually changed. Stale CFU is not allowed.

Also set the matching item's `status: done` in `arrive/implementation-plan.yaml`, then run `arrive plan check`.

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

If `arrive score` is Red (>60), either split the work or add a `## Reviewability` section to the advance justifying why the change is atomic. Say which you chose.

If any check fails and you cannot fix it, push what you have on the branch and report the failure plainly. Do not disable a test, relax a lint, or weaken an assertion to get green — that defeats the point of the gates.

## Step 8 — Push and open a PR

```
git push -u origin advance/<ADV-ID>
```

Then try `gh pr create --title "<ADV-ID>: <title>" --body "..."`. If `gh` is not authenticated, report that the PR needs opening manually and give the compare URL:
https://github.com/srswart/totem/pull/new/advance/<ADV-ID>

Never push directly to `master`. Never force-push.

PR body should include: the advance ID and objective, what changed, the evidence you actually captured, the reviewability score, and anything you could not satisfy.

## Step 9 — Preserve the journey report (mandatory when not gated)

The final report is our record of the journey — it must survive the session. After the PR is opened (or the compare URL determined), write the full final report (the same content as the "Final report" section below) to:

```
docs/agent-journey/<UTC-date>T<UTC-time>Z-<ADV-ID>.md      e.g. docs/agent-journey/2026-08-12T140503Z-ADV-CORE-001.md
```

Start the file with this frontmatter so the reports are analyzable in aggregate later:

```yaml
---
run_at: "<UTC RFC3339>"
advance: "<ADV-ID>"
outcome: complete | partial | failed | blocked
reviewability_score: <n>
checks_passed: [...]
checks_failed: [...]
branch: "advance/<ADV-ID>"
pr: "<PR or compare URL>"
---
```

Then commit it on the advance branch with prefix `docs:` and push again — the open PR updates automatically, so the report merges into master together with the work it describes:

```
git add docs/agent-journey/
git commit -m "docs: agent journey report for <ADV-ID>"
git push
```

This applies to partial and failed runs too — those reports are the most valuable to analyze. If the run failed before a branch existed (e.g. bootstrap failure at Step 2), create the report on a branch named `journey/<UTC-date>-bootstrap-failure`, push it, and note it in your report. Only fully gated Step-1 runs skip this step: they produce no branch and only the GATED line.

## Final report

If gated at Step 1, report only the GATED line — nothing else is needed.

Otherwise end with:
1. The architecture line printed by bootstrap-arrive.sh
2. Which advance you implemented, and whether it is complete or partial
3. Check results — passes AND failures
4. The reviewability score, and whether you split or documented
5. Any correction you made to the advance itself
6. Any invariant you established only partially, stated plainly
7. The branch name and PR link, or the compare URL if gh was unavailable
8. Confirmation that the journey report was committed (its path), per Step 9

Be accurate about failures. A truthful report of a partial run is far more useful than an optimistic one.
