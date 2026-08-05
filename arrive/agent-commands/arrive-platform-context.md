# /arrive-platform-context

Gather platform context relevant to the current task and brief the session.
Member repos only (requires a `platform:` block with `role: member`).

## Steps

1. Run `arrive platform check --json` (add `--no-fetch` if offline). If the
   platform checkout is missing or stale, report that first with the
   remediation the output names — context gathered from a stale platform may
   be wrong.
2. If `context_repositories` is non-empty, run `arrive context sources --json`
   first and note external sources alongside platform artifacts.
3. Read `<platform-path>/arrive/platform/INDEX.md` (the platform path is in
   this repo's `arrive/registry.yaml` under `platform.path`).
4. From the INDEX, select what is relevant to the current task:
   - context artifacts whose `applies_to` includes this repo (or `all`) and
     whose topic overlaps the task;
   - contracts this repo provides or consumes;
   - open plan items assigned to this repo.
5. Read the selected artifacts from the platform checkout (read-only — never
   edit them from here).
6. Brief the session: a short summary of the constraints (`binding` first),
   relevant guidance, contract versions in play, and open plan items. Cite
   artifact ids and paths so the user can open them.

## Notes

- Deterministic data comes from the CLI and the platform tree; this command
  adds selection and synthesis judgment only.
- If nothing in the platform is relevant to the task, say so explicitly — do
  not pad.
