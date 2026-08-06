---
advance:
  id: "ADV-CORE-004"
  title: "Investigation: value-attribution proxy experiment"
  system: "058-totem-core"
  primary_component: "core"
  components: ["core"]
  started_at: "2026-08-05T04:39:35Z"
  implementation_completed_at: "2026-08-06T10:05:27Z"
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 15
  risk_flags: []
  evidence: ["profile:selected-practices", "investigation:findings"]
  practices:
    tidy_first:
      status: not_applicable
      rationale: "Investigation mode, no production_code work product — no code was written, only a findings document."
    tdd:
      status: not_applicable
      rationale: "Investigation mode, no production_code work product. The 'mattered' labels were derived from reading existing code behaviour, not from tests written ahead of an implementation; no tdd:red-green is claimed."
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software, quality]
  work_products: []
  status: complete
---

## Objective

Experiment to close the §9 open question "value attribution depth": how far
to chase "did this memory change the outcome?" in v1. The brief names this a
key risk ("usage metering is easy, value attribution is hard"). Evaluate
candidate proxies — retrieval, citation in the following turn, explicit
feedback, outcome linkage — on recorded sample sessions, and pick the v1
signal set and per-category weights for ADV-CORE-002.

## Outcome

- **Decision:** v1 `value_score` for `ADV-CORE-002` leads with the citation
  signal (precision 0.80, recall 0.67 on the labeled corpus), adds
  `provenance.derived_from` graph linkage as a high-precision booster
  (precision 1.0, recall 0.92) once the gateway exposes it — it currently
  does not (`SaveRequest`/`totem_save` have no `derived_from` field, a
  concrete gap this investigation surfaced, VAL-004) — and does **not**
  weight raw retrieval/`use_count` into `value_score` (precision 0.71 at
  perfect recall: necessary, not discriminating). Per-category: the citation
  + derived_from scheme applies to Knowledge and Instructions (where this
  corpus's data actually sits); Context should weight freshness/retrieval
  only (its 12h TTL leaves no time for citation to accumulate); Episodic is
  inapplicable by design (append-only, never value-ranked); Identity and
  Uncertainty have no data in this corpus and are deferred, provisionally
  treated like Knowledge.
- Both named failure modes were observed for real, not hypothesized:
  **citation without use** (TD-008/TD-009, correctly cited as forward
  pointers with no live-query code yet to be right or wrong about) and
  **use without citation** (TD-005, MCP-002, MCP-003 — code correctly
  follows the finding, nobody wrote the finding's id down).
- **Deferred signal:** explicit feedback (`totem_feedback`). Zero data
  points — the tool is `ADV-GATEWAY-004`, still `status: planned`. Trigger
  to revisit: once that advance ships and enough real feedback events
  accumulate to label a comparable-sized corpus (≥17).

Full findings, the labeled dataset, and scoring detail:
[docs/tech-direction/value-attribution.md](../../../../docs/tech-direction/value-attribution.md).

## Planned Work

- [x] branch: create or confirm feature branch for this advance
- [x] collect/construct sample sessions with known "memory actually mattered" labels
      — 17 (finding → consuming-advance) pairs drawn from this repo's own
      `docs/tech-direction/*.md` findings and the code/advances that
      consumed each, labeled by direct code inspection rather than
      invented scenarios
- [x] score candidate proxies against the labels — retrieval, citation,
      outcome-linkage scored; explicit feedback has zero data points
      (`totem_feedback` does not exist yet, `ADV-GATEWAY-004`)
- [x] write findings + v1 signal set to docs/tech-direction/value-attribution.md

## Bug Fixes

- [ ] None yet

## Risk + Rollback

- Risk: hand-labeled samples bias the proxy choice; treat the v1 set as
  provisional and re-run once real feedback data flows.
- Rollback: n/a — findings only.

## Evidence

- [x] profile:selected-practices — investigation mode, empty `work_products`;
      `tidy_first` and `tdd` recorded `not_applicable` with rationale in
      frontmatter. No `tdd:red-green` claimed.
- [x] investigation:findings — docs/tech-direction/value-attribution.md,
      VAL-001…VAL-005, each tied to a labeled corpus row or a direct code
      inspection, not an argued claim.
- Not claimed: `ci:passed` — no pipeline result observed for this branch by
  the time this record was written.
- Not claimed: any measurement from live agent telemetry — no
  `AccessLogEntry` recall data or `totem_feedback` events exist in this repo
  yet; every proxy score is retrospective (see doc §2, §5).

## CI Evidence Notes

- Findings document is the primary artifact; no pipeline evidence expected.

## Changes Made

### 2026-08-06 - feat: value-attribution proxy findings
- docs/tech-direction/value-attribution.md: new — verdict, VAL-001…VAL-005,
  the 17-row labeled dataset, the method (real self-referential corpus, not
  invented sessions), and the v1 signal set / per-category weights
  recommendation for ADV-CORE-002

### 2026-08-06 - docs: complete ADV-CORE-004
- arrive/systems/058-totem-core/advances/ADV-CORE-004.md: status complete,
  evidence, practice dispositions, refreshed Outcome and CFU
- arrive/implementation-plan.yaml: plan item ADV-CORE-004 set to done

## Check for Understanding

1. Row #13 (MCP-001's secondary finding) is the one case where R=Y, C=N, and
   O=N simultaneously. Why does that combination make it the single most
   important row in the dataset, and what does it demonstrate that no
   citation-scoring exercise alone could catch?
2. The doc computes outcome-linkage's precision as 1.0 (11/11) but recall as
   only 0.92 (11/12). Which row accounts for the gap, and why does that gap
   — not the precision number — say the most about why outcome-linkage
   can't be v1's *only* signal?
3. `provenance.derived_from` already exists in `crates/totem-core/src/provenance.rs`
   and `crates/totem-store/src/schema.rs:73`. Find where `SaveRequest`
   (`crates/totem-gateway/src/dto.rs`) and `totem_save`'s MCP parameters
   (`crates/totem-gateway/src/mcp.rs`) are defined — what's actually missing
   before an agent could ever populate that field on a real write?
4. Rows #6/#7 (TD-008, TD-009) and rows #4/#14/#15 (TD-005, MCP-002, MCP-003)
   are each scored C=Y-but-M=N or C=N-but-M=Y respectively. Pick one pair
   from each group and explain, from the actual cited code, why the citation
   signal got it wrong in opposite directions.
5. The advance's own Risk section says hand-labeled samples bias the proxy
   choice. This investigation used a real corpus instead of invented
   sessions — does that resolve the risk or just relocate it? Point to the
   specific line in the findings doc that names the honest answer.
6. Categories Identity and Uncertainty have zero rows in the labeled
   dataset. What does the Outcome section recommend doing about their
   per-category weight in the meantime, and what would have to be true
   before that recommendation should be revisited?
