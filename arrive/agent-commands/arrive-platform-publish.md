# /arrive-platform-publish

Draft and publish a dispatch for an advance in this member repo.
Propose-and-confirm — never publish without the user agreeing.

## Steps

1. Identify the advance: the one named by the user, else the active
   work-context advance (`arrive work` / `.arrive/work-context.json`).
2. Read the advance and check its dispatch-feeding metadata:
   - Does the change touch a contract this repo provides or consumes? If yes
     and `contracts_touched` is missing from the advance frontmatter, propose
     adding it (list the contract ids).
   - Does it progress a platform plan item? Propose `plan_items` likewise.
   - Is the Objective's first paragraph an accurate one-line summary? It
     becomes the dispatch `summary` — tighten it in the advance if not.
3. Show the user what will be published (advance id, title, status, contracts,
   plan items) and confirm.
4. Run `arrive platform publish --advance <ID>`. Relay the output:
   - checkout path: the suggested `git add`/`commit` commands for the platform
     checkout (or add `--commit` if the user asks);
   - cache path: the pushed `dispatch/<member>/<id>` branch and the PR step.
5. If publish reports the platform is unreachable and no `url` is configured,
   surface the named error and its remediation — do not improvise.

## Notes

- Re-publishing an unchanged advance is a safe no-op (idempotent per
  member + advance); re-publish after substantive advance updates.
- The dispatch content is deterministic CLI output — your judgment goes into
  the advance (summary, tags) before publishing, not into the dispatch file.
