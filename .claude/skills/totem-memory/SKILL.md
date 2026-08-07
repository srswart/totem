---
name: totem-memory
description: How to use Totem's shared memory in this repo — when to recall, what is worth saving, which category and scope to use, and what never to save. Load when starting work that might benefit from prior context, before saving a memory, or when unsure whether something belongs in Totem at all.
---

# Using Totem

Totem is this project's shared, durable memory: `https://totem-dev.fly.dev`.
Every session that works on Totem — workstation Claude Code, the hourly cloud
routines, Cursor — reads and writes the same store, so what you save is what
the next session (yours or someone else's) starts with.

This skill is the detail. The short version lives in `CLAUDE.local.md` and in
`docs/cloud-agent-notes.md`, because a rule that costs context in every
session has to stay short.

**Status: this is v1 of a trial.** The discipline below is a starting
hypothesis, not settled practice. If it produces noise, that is a finding —
record it in `docs/overnight-experiment/log.md` rather than quietly ignoring
the rule.

## The two moments that matter

**Recall before you read.** When you start on an advance, or touch a
component you have not touched this session, recall first. Query the advance
id and the component names. It costs one call and may save you re-deriving a
decision someone already made — or repeating a dead end someone already hit.
Recall *before* reading the governance docs, not after: if a memory
contradicts a doc, that contradiction is the most valuable thing you will
learn all session, and you want to notice it rather than have the doc frame
your reading.

**Save what the next session needs.** At the end of a piece of work, ask:
*what did I learn that is not already written down anywhere?* Save that.
Nothing else.

## What is worth saving

- **A decision and its reasoning** — especially one that closed off
  alternatives. "We chose X over Y because Z" survives; "we chose X" does
  not help the next person weigh a change.
- **A dead end.** The thing you tried that did not work, and why. This is the
  single highest-value category and the one most often skipped, because
  finishing something feels more worth recording than failing at something.
  The next session will try the same dead end otherwise.
- **A correction to a spec or a doc.** When an advance, a tech-direction
  record, or a comment turned out to be wrong, save the correction — the
  document may be fixed later, but the *fact that it was wrong* is what stops
  someone trusting its neighbours.
- **A constraint discovered by running something.** "The SDK closes the
  engine asynchronously", "connectors cannot carry a static bearer" — things
  no amount of reading would have revealed.

## What never to save

- **Anything git already records.** What changed, who changed it, when. The
  diff is authoritative and Totem is not a second copy of it.
- **Anything ARRIVE already records.** Advance status, reviewability scores,
  plan sequencing, component ownership. Those live in `/arrive/` and the
  landscape already ingests them.
- **A summary of what you just did.** If it is derivable from the commits and
  the advance record, it is noise. The test: would this memory change what a
  future session *does*, or would it merely tell them what happened?
- **Secrets, tokens, credentials.** Ever. Under any category.
- **Speculation about the future.** Plans belong in the plan.

## Categories

| Category | Holds | Example |
|---|---|---|
| `knowledge` | Durable facts about the system | "The gateway owns the store exclusively; a second process cannot open it" |
| `context` | Situational state that will expire | "Phase-011 is in flight; the cutover waits on CLI auth" |
| `instructions` | How we do things here | "Run `arrive plan check` before pushing plan edits" |
| `episodic` | What happened, append-only | Session records; rarely written by hand |
| `uncertainty` | Contested or unresolved | Something two sessions disagree about |
| `identity` | Who an actor is | Rarely written by hand |

If a memory is `context`, it will go stale — say *when* it applies inside the
body ("as of phase-011"), so a later reader can judge it rather than trust it.

## Scope

- `actor:<you>` — private to one identity. Notes to your future self.
- `project:srswart/totem` — **the default for this work.** Shared by every
  session on this repo, human and agent.
- `team:<id>` — a team's shared memory.
- `platform` — visible to every enrolled actor everywhere. Requires a
  platform-bound credential and, for promotion, a human decision. Do not
  reach for it.

When in doubt, write to `project:` and let promotion widen it later. A memory
in too narrow a scope is invisible; one in too wide a scope is noise for
everybody, and promotion is the sanctioned path between them.

## Feedback closes the loop

When a recalled memory actually changed what you did — or misled you — say
so with `totem_feedback`. The value loop (ADV-CORE-002) ranks by whether
memories get used and help, and it has no other way to learn. An unused
signal is a ranking that never improves.

## Calling the tools

Every call asserts an identity that must match your credential's binding.
Workstation sessions authenticate with a bearer token; claude.ai routines
authenticate through WorkOS AuthKit, and their actor is the AuthKit subject.

```
totem_recall  { project, actor, query?, categories?[] }
totem_save    { project, scope, category, body, tags[],
                author: { kind: "agent"|"human", actor }, harness, session }
totem_feedback{ memory_id, signal, ... }
```

`author` must be an **object**, not a JSON string — and if a harness
stringifies it anyway, the gateway now parses it (ADV-GATEWAY-013). Recall
responses deliberately omit embedding vectors (ADV-GATEWAY-014); if you need
one, read the store directly.

## Connecting

- **Workstation Claude Code:**
  `claude mcp add --transport http totem https://totem-dev.fly.dev/mcp --header "Authorization: Bearer <token>"`
  The token lives at `~/.totem/bootstrap-token` on Shawn's machine; issue
  per-identity credentials as they become available.
- **claude.ai routines:** a custom connector at
  `https://totem-dev.fly.dev/mcp`, authenticated through WorkOS AuthKit.
  Dynamic Client Registration works; leave the OAuth fields blank.
- **Cursor:** **unverified.** `docs/tech-direction/mcp.md` records Cursor's
  remote-MCP reach as untested (the sandbox could not reach its docs), and
  nothing has tested it since. Do not assume it works because the others do.
