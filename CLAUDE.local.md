# Repo-specific instructions

ARRIVE never writes this file (see `CLAUDE.md`), which makes it the right
home for rules that are ours rather than the kit's — and it is committed, so
it travels with a clone rather than living on one machine.

## After opening a PR, check it (2026-08-08)

Opening a PR is not the end of the task. Before reporting it as done, and
again before it merges:

```sh
gh pr view <N> --comments
gh api repos/srswart/totem/pulls/<N>/comments --jq '.[] | "\(.path):\(.line) \(.body)"'
gh run list --workflow=ci.yml --limit 1   # CI, not just the local check
```

**Why this is written down.** The cloud routine does this as a protocol step;
the workstation session did not, and review feedback was only ever acted on
when Shawn pointed at it. Copilot's comments on #77 were all five valid, and
one of them — a test that could not observe the property it claimed to assert
— was the same class of defect the session had spent an advance learning to
find.

Related: **CI green and local green are different claims.** CI was failing for
two days across several merges while the workstation reported "clippy clean",
because `cargo clippy --workspace --all-targets` piped to grep re-lints
nothing for unchanged crates. Run the canonical
`cargo clippy --workspace --all-targets -- -D warnings` and check the exit
code, and look at the CI run before calling a branch green.

This lives here rather than in Totem deliberately. It is a rule that must
apply every time, so it cannot depend on a retrieval succeeding — the
always-on floor argued for in
`docs/tech-direction/context-delivery.md` (CTX-003). When retrieval is
trustworthy enough to move it, that will be a real test of the direction.

## Totem memory (trial, v1)

This project uses its own product: a shared, durable memory at
`https://totem-dev.fly.dev`. Workstation sessions, the hourly cloud routines,
and any other harness read and write the same store, so what one session
saves is what the next one starts with.

**Four rules. The `totem-memory` skill has the detail — load it before
saving, or when unsure whether something belongs in Totem at all.**

1. **Recall before reading.** Starting an advance, or touching a component
   you have not touched this session: recall first, on the advance id and the
   component names. A memory that contradicts a governance doc is the most
   valuable thing you will find, and you want to notice it before the doc
   frames your reading.
2. **Save what the next session needs, not what you did.** Decisions and
   their reasoning, dead ends, corrections to specs, constraints discovered
   by running something. Never a summary of the diff — git has that, and
   ARRIVE has the advance record.
3. **Default to `project:srswart/totem` scope.** Too narrow is invisible;
   too wide is noise for everyone. Promotion is the sanctioned path between.
4. **Give feedback when a memory helped or misled.** The value loop ranks by
   use and outcome, and has no other way to learn.

**Nothing is packaged yet.** These rules and the skill are deliberately
independent of `arrive/agent-rules/` while we find out what works — whether
they eventually belong in the kit is a decision for after the trial, not
before it (ADV-INFRA-004). If the discipline produces noise, that is a
finding for `docs/overnight-experiment/log.md`, not something to quietly
ignore.
