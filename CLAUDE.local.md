# Repo-specific instructions

ARRIVE never writes this file (see `CLAUDE.md`), which makes it the right
home for rules that are ours rather than the kit's — and it is committed, so
it travels with a clone rather than living on one machine.

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
