# ARRIVE Advance Draft

Generate or update an Advance file that documents the current change.

## Instructions

1. **Understand scope** from the conversation requirements and the current change:

```bash
arrive status
arrive score
```

2. **Render the canonical advance template** and use its `content` as the base — do not invent the structure (template-first authoring). For v2 profiles, pass explicit metadata:

```bash
arrive template render --kind advance --mode implementation --facet software --work-product production_code --json
```

Legacy repos without v2 capability use the default template; inferred software defaults are read-only guidance.

3. **Author the advance from the requirements**, writing to `arrive/systems/<system>/advances/ADV-<COMPONENT>-NNN.md`. Fill Objective / Behavioral Change / profile-appropriate sections / Risk + Rollback / Evidence from the conversation. Set `mode`, `facets`, and `work_products` when the kit supports v2 profiles (`arrive advance enrich --preview` can propose metadata for legacy advances). (`arrive draft` is a scaffold/locator fallback and will **not** overwrite an existing advance.)

   When durable external traceability is relevant, use `external_refs` with a
   configured instance ID, string object ID, ARRIVE relation, optional kind,
   and only a validated explicit URL override. Keep `pr_links` unchanged.
   Run `arrive external-systems check`; never infer evidence, approval, or
   synchronized status from the link and never query the provider.

4. **Set time-tracking fields**:
   - `started_at` when the advance is created
   - `implementation_completed_at` when implementation finishes (or `~` if not done yet)

5. **Honor the org guardrails.** The always-applied `arrive-rules` (rendered by `arrive render --agent`) are **binding**: blocking/mandatory rules must hold. When implementing, run `arrive check --at pr` and `arrive advance attest`; if a blocking rule cannot be satisfied, **stop and surface it** (propose a time-boxed waiver) rather than violating it.

6. **If the advance already exists** (planned status):
   - Read the existing advance
   - Refine sections that need it
   - Don't overwrite the user's custom content

## Drafting Guidelines

### Objective
- One sentence explaining WHY this change exists
- Focus on the problem being solved, not the solution

### Behavioral Change
- Describe what's different AFTER this change ships
- Use "After this advance:" bullet format
- Be specific about observable changes

### Implementation Tasks
- Break into logical phases aligned with work products (tidy/test/feat for `production_code`; control validation for `test_automation`; provenance/reset for `test_data`; workload/threshold integrity for `performance_harness`)
- Mark completed items as done

### Risk + Rollback
- Identify what could go wrong
- Describe how to undo the change
- Note any dependencies or migration concerns

### Evidence
- List verification methods appropriate to declared work products
- Include test types (unit, integration, manual) when relevant
- Note TDD/Tidy only when `production_code` applies; use automation control validation otherwise
- Keep evidence separate from `external_refs`; a `verifies` link does not prove an external execution passed

## Expected Output Format

```
📝 Advance Draft

Created/Updated: arrive/systems/[system]/advances/ADV-[COMPONENT]-NNN.md

Summary:
├─ ID: ADV-[COMPONENT]-NNN
├─ Title: [descriptive title]
├─ System: [system-id]
├─ Components: [list]
└─ Score: XX [LEVEL]

Sections:
✓ Objective - [summary]
✓ Behavioral Change - [N bullet points]
✓ Implementation Tasks - [N tasks]
✓ Risk + Rollback - [identified]
✓ Evidence - [N items]

💡 Review the advance file and refine as needed.
```
