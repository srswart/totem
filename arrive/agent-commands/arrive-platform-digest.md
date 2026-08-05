# /arrive-platform-digest

Synthesize a change narrative across the cluster from dispatches.
Requires a `platform:` block. Run from the platform repo or a member — the CLI
resolves the platform checkout from members and prints a scope banner.

## Steps

1. Gather deterministic data: `arrive platform digest --json` (add `--since
   <date>` / `--member <id>` / `--contract <id>` per the user's question).
2. Synthesize a narrative grounded strictly in that data:
   - group by member repo, then by theme (contract changes first, then plan
     progress, then other changes);
   - call out dispatches with `contracts_touched` — name the contract and the
     consumers that will need to absorb;
   - call out `plan_items` progress against the platform plan;
   - flag risk (`risk_flags`, non-`complete` statuses) explicitly.
3. Cite advance ids and member repos inline so readers can trace every claim
   to a dispatch.
4. If the user asks "what changed since X", state the window you queried and
   how many dispatches it covered.

## Notes

- The CLI provides listing/filtering only; the narrative is your work — keep
  it faithful to the dispatch data, no invented detail.
- Empty result sets are an answer: say "no dispatches in that window", don't
  pad.
