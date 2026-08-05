# Platform Stewardship

This repository is a **platform** — it owns cluster-level coordination artifacts
under `arrive/platform/` for its member repos. Behavioral rules for working here:

## Steward inbound dispatches

- Dispatches under `arrive/platform/dispatches/<member-id>/` are
  machine-generated change digests from members. Review them for contract
  impact (`contracts_touched`) and plan progress (`plan_items`); never
  hand-edit a dispatch — corrections happen by the member re-publishing.
- Use `arrive platform digest --json` for deterministic listing; synthesizing
  digests into a change narrative ("what changed across the cluster") is agent
  work — do it when asked, grounded in the dispatch data.

## Maintain coordination artifacts

- Context artifacts (`arrive/platform/context/`) carry frontmatter (`id`,
  `kind`, `authority`, `applies_to`). Keep `authority: binding` **exceptional
  and justified** — binding artifacts constrain every member repo; default to
  `advisory`. A binding artifact must never weaken a higher-tier org policy.
- Contract changes are ordinary advances in this repo: bump the version,
  append a changelog entry naming the driving dispatch/advance, and expect
  consumers to absorb via their own acknowledgement bumps.
- Keep the cross-repo plan's dependencies honest — `arrive platform plan check`
  validates structure; sequencing decisions are yours to review.

## Keep the INDEX fresh

- After any change under `arrive/platform/`, run `arrive platform index` and
  commit the regenerated `INDEX.md` in the same change. The pre-commit hook
  and CI (`arrive platform index --check`) enforce this — a stale INDEX
  misleads every member's agents.

## Ownership boundary

- This repo governs only itself. Never create components, selectors, or
  advances for member files; member code changes belong in member repos
  (reaffirms the single-repo governance decision).
