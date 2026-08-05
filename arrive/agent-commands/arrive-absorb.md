# ARRIVE Absorb

Bring an existing repository under ARRIVE governance by scanning it and generating an initial draft of ARRIVE artifacts (systems/components) plus a report of unknowns.

## Instructions

1. **Choose identifiers** (fill these in):

- `REPO_ID`: kebab-case stable id (e.g., `acme-payments`)
- `REPO_NAME`: human-friendly name (e.g., `Acme Payments`)
- `DEFAULT_BRANCH`: usually `main`

2. **Run the absorb draft**:

```bash
arrive absorb draft --repo-id REPO_ID --repo-name "REPO_NAME" --default-branch DEFAULT_BRANCH
```

3. **Review outputs**:

- `arrive/registry.yaml`
- `arrive/systems/**/system.yaml`
- `arrive/systems/**/components/*.yaml`
- `arrive/systems/**/advances/*.md` (one “absorb” advance per component)
- `arrive/absorb/report.md` (created files + unknowns checklist)

## Notes

- Safety: the absorb workflow refuses to overwrite existing files (especially advances).
- If `arrive/` already exists, rerun with `--force` only if you intend to regenerate drafts (still no overwrites).
- Absorbed advances default to legacy software guidance; use `arrive advance enrich --preview` later to add v2 profile metadata when appropriate — do not assume every component produces `production_code`.
- Do not scrape ticket URLs, credentials, or provider payloads during absorb. Add a reviewed credential-free `arrive/external-systems.yaml` and stable `external_refs` later, then validate offline with `arrive external-systems check`.
