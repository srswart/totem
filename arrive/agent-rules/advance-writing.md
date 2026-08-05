# Advance Writing Rules

Advances are outcome records, not full specifications. Governance posture: `arrive-core.mdc`.

## Frontmatter Schema — Template-First (do not hand-author)

**Never hand-write advance frontmatter from memory.** Generate the file from the template so its structure is always correct:

- `arrive template render --kind advance --json` (use the CLI `content` as the base), or the `arrive advance` generator.
- This produces the canonical **single nested `advance:` block**. Do not invent fields or a flat layout.

The canonical frontmatter is **one `advance:` mapping** — every field nests under it:

```yaml
---
advance:
  id: "ADV-COMPONENT-001"          # ADV-<PRIMARY_COMPONENT>-<SEQ:3>
  title: "Human-readable title"
  system: "system-id"              # the system id (NOT `system_id`)
  primary_component: "component-id"
  components: ["component-id"]      # real component ids from arrive/systems/<system>/components/
  started_at: "2026-01-11T09:00:00Z"
  implementation_completed_at: ~    # ~ until done
  review_time_estimate_minutes: 0
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []                 # optional vendor-neutral traceability
  reviewability_score: 0
  risk_flags: []
  evidence: []
  status: planned                   # planned | in_progress | complete | cancelled
---
```

**Hard rules (these prevent schema drift):**

- The top-level key is **`advance:`** and is **nested**. Do **not** use flat top-level keys.
- ID/system fields are **`advance.id`** and **`advance.system`** — **never** `advance_id` / `system_id`.
- `status` is exactly one of **`planned | in_progress | complete | cancelled`**. **Never use `done`** — `done` is implementation-plan *item* vocabulary, not advance status (`complete` is the advance equivalent). **Never use `completed`** — that word belongs only in timestamp field names such as `implementation_completed_at`, not in `status`.
- **Platform members only** (repo has a `platform:` block): two optional list fields feed the dispatch published by `arrive platform publish` — `contracts_touched` (platform contract ids this advance affects) and `plan_items` (platform plan item ids it progresses). Omit them entirely when empty; never invent ids — use the platform INDEX for the valid sets.
- `pr_links` remain first-class. Optional `external_refs` use configured system IDs plus string object IDs; never replace PR links automatically.
- All metadata (`components`, `status`, `evidence`, timestamps, etc.) lives **in the `advance:` frontmatter**, never as a separate body section.
- `components` and `primary_component` must reference **component ids that actually exist** under `arrive/systems/<system>/components/`. Do not invent component names.

`arrive doctor artifacts` flags advances that do not parse against this schema; `arrive info` warns when it must skip one. If either reports an advance, fix its frontmatter to the block above.

## External Traceability

Use the shared provider-neutral shape only when a durable external relationship
helps readers:

```yaml
external_refs:
  - system: jira-emirates
    id: ECOM-4281
    kind: story
    relation: tracks
```

- Use a configured instance ID (`jira-emirates`, not generic `jira`) and keep `id` a string.
- Relations are `tracks`, `implements`, `verifies`, `supports`, `reports`, `depends_on`, or `related`.
- Prefer catalog-derived navigation; add an explicit `url` only as a validated override.
- A reference establishes traceability only. It is not test evidence, approval, completion, or synchronized provider status.
- Never put credentials, tokens, cookies, query secrets, personal/test data, or sensitive context in catalogs, URLs, labels, or IDs.
- Validate and inspect offline with `arrive external-systems check`, `list`, and `resolve`; do not call provider APIs while authoring.

## Lite Workflow

Full is the default. In Lite, an implementation plan remains optional. Keep a
present plan aligned with Advances, and ask before promotion to Full.

## Required Sections

Every Advance must include:

### 0. Time Tracking (Frontmatter)

Every Advance must capture when work started and when implementation finished. These fields live **under the `advance:` block** (see Frontmatter Schema above):

```yaml
advance:
  # ...
  started_at: "2026-01-11T09:00:00Z"
  implementation_completed_at: "2026-01-11T09:30:00Z" # or ~ if not done yet
  review_time_estimate_minutes: 20
  review_time_actual_minutes: ~
```

- **Format**: ISO 8601 / RFC 3339 timestamps (use `Z` / UTC).
- **Creation**: When an advance is drafted, `started_at` should be set immediately.
- **Completion**: When implementation is finished (usually when status becomes `complete`), set `implementation_completed_at`.
- **Review-time**: Keep `review_time_estimate_minutes` current during implementation; set `review_time_actual_minutes` after review.

### 1. Objective
What is the goal of this change?

### 2. Behavioral Change
What is now different? Describe the observable outcome.

### 3. Component Impact
Impacted components are declared in **frontmatter** (`advance.components`), with the main one in `advance.primary_component` — not as a body section. Every id must exist under `arrive/systems/<system>/components/`:
```yaml
advance:
  primary_component: "component-a"
  components: ["component-a", "component-b"]
```

### 4. Risk + Rollback
Required if any risk flags are present:
- What could go wrong?
- How do we roll back?

### 5. Evidence
What validates this change?

**Profile-first:** read `mode`, `facets`, and `work_products` (see `arrive-advance-profiles.mdc`). Do not require `tdd:red-green` unless `production_code` is a declared work product.

**Production code (`production_code` work product):**
- `tdd:red-green` - Tests written first, implementation made them pass
- `tidy:preparatory` - Tidying commits preceded feature work

**Test automation:** control validation, oracle, stability — not TDD.

**Test data:** provenance, privacy/residency, setup/reset — synthetic fixtures only.

**Performance:** workload + environment provenance; missing provenance is indeterminate.

**Test evidence:**
- `tests:unit` - Unit tests passing
- `tests:integration` - Integration tests passing
- `tests:e2e` - End-to-end tests passing

**Other evidence:**
- `ci:passed` - CI pipeline passed
- `review:owner` - Owner review completed

Record practice dispositions under `advance.practices` when using schema v2 (`applied`, `not_applicable` with rationale, or `waived`).

When pipeline checks are disabled, record externally-run checks the same way:
- run `arrive pr check --strict --json` before merge
- capture provider/run metadata with `arrive evidence record ...`

### 6. Check for Understanding (CFU)

**Mandatory** (see also `arrive-core.mdc`). On substantive advance updates, add or **fully replace** `## Check for Understanding` in the same editing pass.

**Triggers** (non-exhaustive; if any apply, refresh CFU before ending the turn):

- `## Changes Made` is extended (new files, new commits, or new bullets)
- `implementation_completed_at` is set or changed, or status moves toward complete
- Objective, **Behavioral Change**, **Risk + Rollback**, or **Evidence** are updated to match final implementation
- The narrative is adjusted so it no longer matches an existing CFU block

**Stale CFU is not allowed.** If a CFU section already exists but the scope or story of the advance changed, **replace the entire `## Check for Understanding` block** with a fresh set of questions for the **final** objective, behavior, risk, and **latest** `Changes Made` paths. Do not leave old questions that reference superseded work.

```markdown
## Check for Understanding
```

CFU questions must be advance-driven:

- source topics from Objective, Behavioral Change, and Risk + Rollback
- ground questions in files changed under that advance (prefer `Changes Made` paths)
- avoid generic repo-wide questions not tied to this advance

**How to generate:** In-editor (AI-authored) content is fine and often preferred. The ARRIVE CLI is optional:

- `arrive advance mark-implementation-complete <ADV-ID>` attempts CFU when the CLI build supports it
- `arrive advance cfu <ADV-ID> --refresh` — if a section already exists and you use the CLI to replace it, you may need `--reroll` (or `--seed`) so generation is not skipped

If CFU quality is weak, **rewrite in place** or re-run the CLI for that advance until the questions match the change.

## Development Practice Compliance

Advances should document adherence to Tidy First and TDD:

### Commit Sequence
The advance should reflect commits in order: `tidy → test → feat/fix`

### TDD Compliance
- [ ] Tests were written before implementation
- [ ] Tests failed initially (red phase documented)
- [ ] Implementation made tests pass (green phase)

### Tidy First Compliance
- [ ] Preparatory refactoring in separate commits
- [ ] No behavior changes in tidy commits
- [ ] Tidying labeled with `tidy:` prefix

## Tech Direction References

If a tech direction applies to this advance:
- Reference it in the advance header or objective
- Note which decisions (TD-001, TD-002, etc.) guided the implementation
- Flag any deviations from the tech direction

**Example:**
```markdown
## Objective

Implement status engine per [Tech Direction: Server Sync](../../docs/tech-direction/server-sync.md).

Follows:
- TD-005: Core types as contract (all types derive Serialize/Deserialize)
```

## Cursor Plan Integration

If a Cursor Plan exists:
- Attach it as "input" or reference
- Don't let it replace the outcome narrative
- The Advance summarizes WHAT was done, Plan showed WHAT TO DO

## Advance ID Format

Use: `ADV-{PRIMARY_COMPONENT}-{SEQ:3}`

The primary component is the main component being changed (uppercase, abbreviated if needed).

**Examples:**
- `ADV-STATUS-001` - First advance for status-engine component
- `ADV-AUTH-002` - Second advance for auth component
- `ADV-REVIEW-003` - Third advance for reviewability component

**Naming conventions:**
- Use uppercase for component abbreviation
- Keep abbreviations recognizable (STATUS, AUTH, REVIEW, CLI, HOOKS)
- Sequence is per-component (each component has its own counter)
- File path: `arrive/systems/<system>/advances/ADV-<COMPONENT>-<SEQ>.md`

## Changes Made (Execution Log)

When a plan is executed, append a "Changes Made" section to the advance:

```markdown
## Changes Made

### <Date> - <Commit Prefix>: <Summary>
- <file>: <what changed>
- <file>: <what changed>

### <Date> - <Commit Prefix>: <Summary>
- <file>: <what changed>
```

**Example:**
```markdown
## Changes Made

### 2026-01-11 - tidy: Extract UserInput type
- core/src/types.rs: Added UserInput struct
- core/src/handler.rs: Removed inline type definition

### 2026-01-11 - test: Add email validation tests
- core/src/validation_test.rs: Added test_email_empty, test_email_invalid

### 2026-01-11 - feat: Implement email validation
- core/src/validation.rs: Added validate_email function
```

This log serves as the "what actually happened" record, complementing the objective (what we intended).

## Workflow Checklist Expectations

When creating/updating an advance, include and maintain:

- A branch task in `## Planned Implementation Tasks` (create/confirm feature branch for the advance)
- A `## Bug Fixes` section for defects discovered/fixed during execution
- A `## Changes Made` section with dated incremental updates as work progresses
- After substantive updates: **`## Check for Understanding` is present, matches final scope, and was refreshed if the advance content changed** (see §6)
