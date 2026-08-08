---
advance:
  id: "ADV-STORE-009"
  title: "The calibration corpus becomes a versioned artifact, not compiled-in code"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store"]
  started_at: "2026-08-08T12:40:00Z"
  implementation_completed_at: "2026-08-08T13:20:00Z"
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 23
  risk_flags: []
  evidence: ["tests:unit", "tests:integration"]
  model_usage: []
  schema_version: 2
  mode: implementation
  facets: [software]
  work_products: [production_code]
  status: complete
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

## What shipped, and the one result that matters

`corpora/calibration-v1.json` — 30 records, 9 golden queries, one manifest
with a SHA-256 over both. Loaded by `crates/totem-store/src/calibration.rs`,
which also seeds it and runs its queries.

**Format: one JSON document.** Records and queries had to live together, and a
line-oriented format cannot hold a manifest plus two heterogeneous
collections readably. `serde_json` and `sha2` were already in the tree via
surrealdb, so this added no compilation — though the manifest change itself
costs one dependency-cache bust on the next deploy (ADV-INFRA-008).

**The result worth recording: the corpus scores 7 of 9, and both failures are
ADV-GATEWAY-016's defect.**

```text
FAIL relevance_outranks_category_within_a_topic
       top was "Rocket rule: a production deploy needs a second approver",
       wanted deploy-rollback
FAIL an_unrelated_topic_does_not_intrude
       deploy-approval present but must not be
```

Both are an `Instructions` record beating a more relevant `Knowledge` one on
`category_weight`. **That defect previously required a deployed gateway and
the real embedder to observe.** It is now reproducible in an in-process test
against the deterministic embedder, in under a second.

That is the point of the artifact, and it is the inverse of what came before:
ADV-CORE-008's corpus scored a perfect 1.0 against a ranker that ignored the
query entirely. A corpus that passes against today's ranker would be
reproducing the defect it exists to expose.

**The suite therefore asserts executability, not score.** Turning these green
belongs to the ranking fix and to ADV-CORE-005; a test that demanded 9/9 here
would have to be written *after* the fix, which is how a fixture ends up
describing the implementation instead of the requirement.

### A false alarm the harness produced, and what it changed

The first scoring run reported `leak-bait-juniper present but must not be` —
reading as a **scope-isolation leak**, the most serious class of defect this
system has.

It was not one. Only one record came back, at the reader's own scope with the
reader's own provenance; the other actor's copy was correctly invisible. The
harness compared results to expectations **by body**, and the leak-bait pair
is byte-identical at two private scopes *by design* — that is exactly what
makes it prove isolation rather than prove the embedder can read. Matching on
body alone reported the reader's own copy as somebody else's.

Fixed by identifying records by body **and scope**. Recorded because a false
security alarm is worse than a missed one: it costs the credibility of every
later report, and `corpus.rs`'s own doc comment had already said the check
must be on "the *count* and the *provenance*", not the body. The warning was
there and this harness did not heed it.

### Re-stamping

The checksum is computed by Rust and stamped by an `#[ignore]`d test:

```sh
cargo test -p totem-store --test calibration -- --ignored restamp
```

A test rather than a `[[bin]]` deliberately — cargo auto-discovers binaries
as targets, so a new one would change `recipe.json` and cost a full
dependency rebuild on the next deploy (ADV-INFRA-008). It also has to be Rust
rather than the generating script: `serde_json` orders keys by struct field
declaration, and the first artifact was stamped by a Python generator whose
ordering disagreed on the very first run.

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
6. The corpus scores 7 of 9 and the suite passes anyway. Justify that, and say
   what would be wrong with a suite that required 9 of 9 today.
7. The scoring harness reported a scope leak that had not happened. What was
   it comparing, why did the leak-bait fixture defeat it specifically, and why
   is a false positive here worse than a false negative?
8. The checksum must be computed by Rust rather than by whatever writes the
   file. Why?
