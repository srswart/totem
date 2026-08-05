# Capturing Learnings

When you discover **reusable** knowledge during work — a gotcha, a workaround, a pattern, an integration quirk — propose recording it in the learnings ledger so the next person or agent does not rediscover it. **Never write entries silently**; always propose and let the user confirm.

## When to propose

Propose a learning when any of these occur:

- You resolved a problem with a **workaround** (e.g. a required tool could not be installed, an API behaved unexpectedly).
- It took **multiple iterations** to resolve the same issue.
- An advance's **spec assumption did not hold** (something the advance specified turned out to be wrong or unavailable).
- A Confusion-Protocol stop resolved, or your notes/evidence mention "had to retry because…".

## How to propose

- Suggest running `/arrive-learnings-propose`. That command drafts a templated entry from the session context and asks the user to confirm or edit before anything is written.
- The entry is persisted **only** via the deterministic `arrive learnings add` — no silent writes.

## Coordination with the Friction Log

- If the issue is specific to the **current advance**, record it on that advance's **Friction Log** first (in-context).
- Propose a **learning** only when the knowledge is **reusable beyond this advance** (graduation).
- Do not double-prompt: capture friction in-context, then offer a learning only for the reusable, cross-advance takeaway.

## Graduating learnings to the platform (platform members only)

When this repo is a **member** of a Platform ARRIVE cluster and a learning is
relevant **beyond this repo** — an integration quirk with a sibling service, a
contract gotcha, a cluster-wide pattern — propose graduating it to the
platform's cluster ledger (`arrive/platform/context/learnings.md` in the
platform repo):

- Same propose-and-confirm posture: never write to the platform ledger
  silently; the entry lands via a PR in the platform repo.
- Keep the repo-level entry too when it has local value; note the graduation
  in its `See also`.
- The graduation ladder is: Friction Log (advance) → repo learnings ledger →
  platform cluster ledger — each step widens the audience, so each step needs
  the reusability case to be stronger.

## Reading learnings before you work

Before implementing, consult the generated `arrive-learnings` rule (produced by `arrive learnings index`) so you do not recreate a known gotcha or pattern. If your work would repeat one, follow the established approach unless there is a reason to diverge — and note the divergence.
