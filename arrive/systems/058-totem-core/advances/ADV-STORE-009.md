---
advance:
  id: "ADV-STORE-009"
  title: "The calibration corpus becomes a versioned artifact, not compiled-in code"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store"]
  started_at: ~
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: []
  evidence: []
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: planned
---

## Objective

`crates/totem-store/src/corpus.rs` is ~30 records of Rust, seeded only into a
fresh in-memory store, and explicitly documented as "never a shared
deployment". It has served ADV-STORE-005's purpose well. It cannot serve
calibration, for four reasons:

- **It cannot be versioned independently of the code under test.** "Recall
  quality improved" is meaningless if the corpus changed in the same commit,
  and today it necessarily might have.
- **It cannot grow.** A small-to-medium corpus (hundreds of records across
  categories, scopes, and topic clusters) does not belong in a binary.
- **It cannot be changed without a rebuild.** On this project that is tens of
  minutes per iteration (ADV-INFRA-006 helps and does not remove it), which
  is enough friction to stop anyone tuning a corpus.
- **Variants require code branches.** "Re-enroll in a modified state" — the
  whole point of a calibration rig — should be a different file, not a
  different build.

## Behavioral Change

The corpus becomes a **versioned data artifact** with a manifest: a corpus
id, a version, a checksum, and a description of what it is for. Evidence can
then say *"measured against `calibration-v1`"* and mean something checkable.

Two properties are non-negotiable, both learned the hard way:

- **Each record carries its economics** — `value_score`, `use_count`,
  `last_used_at`, `currency`. ADV-CORE-008 proved that a corpus with uniform
  pristine economics **cannot fail**: the three non-relevance terms are
  constant across the estate, cancel, and `eval_quality` scored a perfect 1.0
  against a ranker that ignored the query entirely. A corpus that cannot fail
  is not a test.
- **The golden queries live in the same artifact as the records.** They are
  currently adjacent in one module and will drift the moment they are not.
  Questions and answers version together or the version means nothing.

`corpus.rs`'s existing behaviour keeps working: `seeded_in_memory()` remains
the deterministic in-process reset path that ADV-STORE-005's tests and
`eval_quality` depend on. This advance changes where the data comes from, not
what the tests can do.

## Scope and Boundaries

**In scope:** the artifact format and manifest, the loader, migrating today's
fixtures into `calibration-v1` (including ADV-CORE-008's economics pair),
checksum verification, and growing the corpus to a size worth calibrating
against.

**Out of scope:** seeding it into a deployed instance (ADV-INFRA-007);
running the evaluation over it (ADV-CORE-005); the scorer itself
(ADV-GATEWAY-008, done).

## Design Notes

Format is the implementer's call, recorded with reasoning. JSONL suits
append-and-diff and keeps one record per line for review; YAML reads better
for hand-authoring the golden queries. A single file keeps records and
queries together, which this advance requires — so if the format cannot hold
both readably, that is an argument about the format, not about the
requirement.

The `GENERATOR_TAG` invariant must survive: every synthetic record stays
identifiable, so a corpus that ever leaks into a real instance can be told
apart from a real memory. If anything, the case is stronger once the corpus
can be loaded into a deployment.

**Corpus content is where the real work is.** A large corpus of near-
identical synthetic sentences calibrates nothing. It needs topic clusters
with genuine near-misses — records that *should* lose to a better match and
records that should win despite worse economics — because those are the only
cases where ranking has anything to get wrong. ADV-CORE-008's failure was
found by exactly one such pair.

## Risk + Rollback

- Risk: a corpus loaded from disk can be edited to make an evaluation pass.
  The checksum in the manifest and the version quoted in evidence are the
  guard; a result recorded against an unrecorded corpus version is not
  evidence.
- Rollback: the compiled-in fixtures remain in history; reverting restores
  them.

## Evidence

- [ ] tests:unit — the loader round-trips every field, economics included.
      A silently-dropped `value_score` reproduces the exact defect this
      advance exists to prevent.
- [ ] tests:unit — checksum mismatch is refused loudly, not warned about.
- [ ] tests:integration — `eval_quality` runs against the artifact and still
      distinguishes the positive from the negative control (ADV-GATEWAY-008's
      sensitivity proof must survive the move).

## Check for Understanding

1. `eval_quality` scored 1.0 against a ranker that ignored the query. Name
   the single property of the corpus that made that possible, and say why
   moving the corpus to a file does not by itself fix it.
2. Why must the golden queries live in the same artifact as the records
   rather than alongside it in code?
3. What does a checksum in the manifest protect against that a version number
   alone does not?
4. A corpus of 500 synthetic records could still calibrate nothing. What
   property must its content have, and which ADV-CORE-008 finding is the
   evidence for that claim?
5. Why does `GENERATOR_TAG` matter *more* after this advance than before it?
