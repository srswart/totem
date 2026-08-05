# Reviewability Rules

Keep changes bounded and reviewable. TDD/Tidy First applies to **`production_code`** work products (`arrive-dev-practices.mdc`). Profile-aware practice credit uses declared work products — automation control validation can reduce score without TDD (`arrive-advance-profiles.mdc`). Budget thresholds: `arrive-core.mdc`.

## Smaller, Bounded Edits

Prefer smaller, focused changes:

1. **Isolate tidying** - Refactoring commits separate from behavior changes
2. **Isolate migrations** - Database/schema changes in separate PRs
3. **Isolate auth changes** - Security-sensitive code separately
4. **Separate interface from implementation** - Public API changes vs internal refactors
5. **Split by ARRIVE component** - When touching multiple components

## Review-Time Usage

- Treat review-time estimate as a planning signal, not a vanity metric.
- Use `arrive score` output to decide checkpointing:
  - Green: proceed, optional split
  - Yellow: split at natural tidy/test/feat boundaries
  - Red: split before continuing implementation
- Track estimated and actual review time in the Advance frontmatter and adjust future split strategy when estimates are consistently off.

## Evidence Capture

Always run the fastest verification that matches change type:

| Change Type | Required Evidence |
|-------------|-------------------|
| Production logic change | Unit tests (TDD when work product is `production_code`) |
| API change | Integration tests |
| Test automation | Control validation, oracle, stability |
| Test data | Provenance, reset, synthetic-only fixtures |
| Performance harness | Workload + environment provenance, thresholds |
| UI change | Visual/E2E tests |
| Config change | Lint/validation |
| Tidying (production code) | Existing tests still pass |

Attach evidence to the Advance.

## Risk Signals

Watch for these risk flags:

- `resident_touched` - Resident component modified
- `migration` - Database/schema changes
- `auth` - Authentication/authorization changes
- `concurrency` - Threading/async changes
- `caching` - Cache invalidation changes
- `public_api` - Public interface changes
- `new_dependency` - New external dependency

Each flag increases reviewability score.
