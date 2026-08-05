---
advance:
  id: "ADV-STORE-006"
  title: "Investigation: SurrealDB server-mode parity (embedded vs `surreal start`)"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store"]
  started_at: "2026-08-05T08:59:40Z"
  implementation_completed_at: "2026-08-05T10:03:15Z"
  review_time_estimate_minutes: 25
  review_time_actual_minutes: ~
  pr_links: []  # filled after PR creation
  external_refs: []
  reviewability_score: 41
  risk_flags: ["auth"]
  evidence: ["profile:selected-practices", "investigation:findings", "tests:integration", "automation:control-validation"]
  practices:
    automation_control_validation:
      status: applied
      rationale: "Every new assertion carries a control: wrong-password refusal vs same-connection successful signin; viewer silent-discard vs identical root CREATE persisting (read-back both ways); capability denials vs capability-free baseline passing; HTTP live refusal vs identical statement accepted over WebSocket."
    tidy_first:
      status: not_applicable
      rationale: "Investigation mode; no production code touched — additions are test automation in the spike crate."
    tdd:
      status: not_applicable
      rationale: "test_automation work product, not production_code. Assertions were derived from executed server behaviour (the VIEWER silent-discard finding overturned the expected-refusal assertion); no tdd:red-green is claimed."
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software, security]
  work_products: [test_automation]
  status: complete
---

## Objective

Close the one question [ADV-STORE-004](ADV-STORE-004.md) could not answer: does
every capability verified against the embedded `kv-mem` engine behave the same
against a real `surreal start` server, and do the auth and capability defaults
that genuinely differ between the two change any behaviour Totem depends on?

Findings extend [docs/tech-direction/surrealdb.md](../../../../docs/tech-direction/surrealdb.md)
§5, which currently states parity as *expected but unverified* and carries it as
a residual risk on ADV-STORE-001.

The parity harness already exists — `crates/totem-store-spike/tests/server_parity.rs`,
behind the off-by-default `server-parity` feature. It compiles and has never been
executed. This advance exists because that gap is an *environment* problem, not a
code problem, and burying it inside ADV-STORE-001 would let it be quietly
forgotten.

## Outcome

Executed 2026-08-05 against `surrealdb/surrealdb:v3.2.4` (Docker, started
`start --user root --pass root memory`, i.e. default capability flags) on a
developer workstation with Docker — the cloud sandbox still cannot run a server.

- **H1 confirmed:** all six ADV-STORE-004 experiments pass unchanged over
  WebSocket; the suite now also asserts the server version matches the `=3.2.4`
  pin at run time. 5 tests, 3 consecutive clean runs.
- **H3 refuted (the insurance paid out as "no"):** no isolation-relevant
  divergence — the scope predicate stays inside the `KnnScan` on the server and
  the live feed publishes nothing from rolled-back turns. **The ADV-STORE-001
  residual risk ("parity expected but unverified") is retired.**
- **H2 confirmed with three executed findings**, now TD-009..TD-011 in
  [docs/tech-direction/surrealdb.md](../../../../docs/tech-direction/surrealdb.md) §5:
  live queries refused over the HTTP transport (server text: *"Unable to
  perform the realtime query"*); default-server capability denials for
  scripting and network, with different refusal text than the embedded build;
  and the sharpest one — **a VIEWER-role user's data writes are silently
  discarded** (`CREATE` returns OK, persists nothing, no error anywhere), while
  its DDL fails loudly and its reads span every scope. Least-privilege DB users
  cannot be trusted with error-checked writes, and DB roles contribute nothing
  to scope isolation — both consequences recorded for ADV-GATEWAY-003 and the
  store invariant.

## Environment Prerequisite (why this advance is separate)

This cannot run in the cloud agent sandbox. Re-verified 2026-08-05: the egress
proxy refuses `download.surrealdb.com` and `install.surrealdb.com` with
`CONNECT tunnel failed, response 403`, and the `surreal` name on crates.io is an
unrelated 2019 crate at `0.4.1` — the published `surrealdb` crate is the client
library only, so `cargo install` cannot produce a server either. The only
in-sandbox route left is building the server from git source, which is far
larger than the ~5-minute client build.

It therefore needs a host with either the binary or a container runtime:

```sh
docker run --rm -p 8000:8000 surrealdb/surrealdb:v3.2.4 \
  start --user root --pass root memory
# or: curl -sSf https://install.surrealdb.com | sh -s -- --version 3.2.4
#     surreal start --user root --pass root memory

TOTEM_SPIKE_SURREAL_URL=ws://127.0.0.1:8000 \
  cargo test -p totem-store-spike --features server-parity --test server_parity
```

The server version must match the client's exact pin (`=3.2.4`) — the
ADV-STORE-004 findings are version-specific (query-plan strings, `<|K,EF|>`
operator behaviour), so a version-mismatched run answers a different question.
`TOTEM_SPIKE_SURREAL_USER` / `_PASS` override the `root`/`root` defaults. The URL
must be `ws://`: live queries are unsupported over the HTTP remote protocol.

The matching implementation-plan item is therefore `blocked`, not `planned`, so
neither hourly cloud routine selects it and stalls. It should be moved to
`planned` only by whoever is running it on a capable host.

## Planned Work

- [x] branch: create or confirm feature branch for this advance
- [x] run the existing parity test against `surreal start` 3.2.4 and record the
      outcome verbatim, including server version and capability flags
- [x] extend `server_parity.rs` with the surface only a server has: authenticated
      signin, behaviour under a least-privilege (namespace/database-scoped) user
      rather than root, and capability defaults (functions, network, scripting)
- [x] execute the HTTP-protocol live-query limitation that ADV-STORE-004
      established from SDK source but never ran
- [x] prove sensitivity: a negative control for each new assertion
- [x] update `docs/tech-direction/surrealdb.md` §5 and add TD entries for any
      divergence found (TD-009, TD-010, TD-011)
- [x] state plainly whether the ADV-STORE-001 residual risk is retired — it is
      **retired**, replaced by the three named constraints

## Bug Fixes

- [ ] None yet

## Scope and Boundaries

**In scope:** the WebSocket transport, authentication (root and least-privilege
users), capability defaults, and whether query semantics or query *plans* differ
from the embedded engine.

**Out of scope:** TLS and certificate handling; deployment topology (deferred to
the `infra` component, see `docs/arrive-decomposition-gaps.md`); Totem's own
token model (ADV-GATEWAY-003); HNSW recall quality at corpus scale
(ADV-STORE-005).

## Risk Hypothesis

- **H1 — query semantics are identical.** Expected true: `engine::local` and
  `surreal start` both execute through `surrealdb_core::kvs::Datastore`.
  Divergence here would invalidate every ADV-STORE-004 finding for production,
  which is why it is worth executing rather than arguing.
- **H2 — auth and capability defaults differ.** Expected true in some form; the
  finding is *which*, and which of them ADV-GATEWAY-003 must handle.
- **H3 — the one that would hurt.** An isolation-relevant behaviour differs on
  the server: the scope predicate is no longer pushed into the `KnnScan`, or the
  live feed publishes a rolled-back write. Either would mean the isolation
  invariant is weaker in production than the tests demonstrate. This is the
  hypothesis the advance is really buying insurance against.

## Control Validation

`test_automation` is the declared work product, so sensitivity — not TDD — is
what makes the result trustworthy.

- **Negative control:** every new assertion must be shown to fail when the
  condition it checks is inverted — e.g. signing in as an under-privileged user
  must fail the assertion that expects a successful authenticated read, and
  removing `scope IN $scopes` must still fail the isolation experiments against
  the server as it does against the embedded engine.
- **Positive control:** the six existing experiments pass unchanged against the
  server. That *is* the parity claim.
- **Oracle:** the embedded run is the reference. A divergence is the finding, not
  a flake to be retried — with one known exception: TD-008 (intra-transaction
  live notifications are unordered) means order-sensitivity is engine behaviour,
  so the parity assertions stay order-insensitive.
- **Repeatability:** the server is started fresh in-memory and the test resets
  its own `totem` / `spike_parity` namespace before and after, so runs do not
  depend on operator cleanup.

## Risk + Rollback

- Risk: running against a server version other than `3.2.4` produces findings
  that are not comparable to ADV-STORE-004's. Mitigated by pinning the server to
  the client's exact pin and recording the version in the findings.
- Risk (`auth` flag): the auth assertions are the genuinely new surface, and a
  test that signs in as root and never exercises a restricted user would report
  parity while leaving the ADV-GATEWAY-003-relevant question untouched.
- Risk: a green parity run that checked nothing. ADV-STORE-004 already made a
  missing `TOTEM_SPIKE_SURREAL_URL` fail fast rather than return `Ok(())`; that
  property must survive the extension.
- Risk (accepted): because no cloud routine can run this, it may sit while
  ADV-STORE-001 and later advances build on unverified parity. Accepted
  deliberately — this advance does **not** block ADV-STORE-001, and the residual
  risk stays recorded there and on ADV-GATEWAY-003 until this runs.
- Rollback: findings plus a test-only change. `crates/totem-store-spike` has no
  dependents and `totem-store` derives its own schema.

## Evidence

- [x] profile:selected-practices — investigation mode, `test_automation` work
      product: `automation_control_validation` applied; `tdd` and `tidy_first`
      recorded not_applicable. No `tdd:red-green` claimed. Notably, the executed
      behaviour overturned a written assertion: the VIEWER write test was
      authored expecting a refusal and the server silently discarded the write
      instead — the assertion now pins the observed semantics (TD-011).
- [x] investigation:findings — TD-009, TD-010, TD-011 added to
      `docs/tech-direction/surrealdb.md` §5, each tied to an executed run with
      verbatim refusal text; §5 retitled from "partially closed" to "closed".
- [x] tests:integration — 5 parity tests against `surrealdb/surrealdb:v3.2.4`
      (SDK-reported build `20260803.93ab219`), server started with default
      capability flags; 3 consecutive clean runs; full workspace suite (51
      tests incl. the new embedded capability probe) green alongside.
- [x] automation:control-validation — controls per assertion: wrong-password vs
      successful signin on the same connection; viewer silent-discard vs
      identical root CREATE persisting (existence checked by read-back both
      times); capability denials vs capability-free baseline (`math::abs`)
      passing; HTTP live refusal vs identical statement accepted over
      WebSocket on the same database.
- Not claimed: `ci:passed` — CI has no SurrealDB server; the parity result is a
      workstation run, recorded here and in the findings.

## CI Evidence Notes

- CI cannot produce this evidence today: the workflow runner has no SurrealDB
  server. Do not claim `ci:passed` for the parity result. If a service container
  is added to the workflow later, record it then — and only then.
- Externally-run checks before merge: `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `arrive pr check --strict --json`, `arrive doctor artifacts`,
  `arrive plan check`, `arrive check --strict`, `arrive score`.

## Changes Made

### 2026-08-05 - test: extend the parity harness with the server-only surface
- crates/totem-store-spike/src/lib.rs: capability probes (`PROBE_BASELINE`,
  `PROBE_SCRIPTING`, `PROBE_NETWORK`) and `probe_expression` returning refusal
  text verbatim
- crates/totem-store-spike/tests/server_parity.rs: shared `root_connection`
  helper; run-time server-version pin assertion; four new tests — bad-credential
  signin refusal, VIEWER-role read-parity/silent-write-discard/DDL-refusal,
  default capability denials, HTTP-protocol live-query refusal with WebSocket
  control — each on its own database for concurrency safety
- crates/totem-store-spike/tests/embedded.rs: embedded half of the capability
  comparison (probes refuse because features are compiled out)
- crates/totem-store-spike/Cargo.toml: `server-parity` feature gains
  `surrealdb/protocol-http` for the transport-limitation test only

### 2026-08-05 - docs: close the parity question
- docs/tech-direction/surrealdb.md: §5 rewritten as "closed (ADV-STORE-006)"
  with the run table, H1/H2/H3 dispositions, TD-009..TD-011, and the retired
  ADV-STORE-001 residual risk; header verdict updated
- arrive/systems/058-totem-core/advances/ADV-STORE-006.md: completed as this file
- arrive/implementation-plan.yaml: ADV-STORE-006 item `blocked` → `done`

## Check for Understanding

1. The VIEWER-role test was written expecting the server to *refuse* an
   under-privileged `CREATE`, and the executed run showed something different.
   What actually happens, how does the test now prove it (two directions of
   evidence), and why does TD-011 matter to ADV-GATEWAY-003's least-privilege
   token design?
2. TD-009 records two different refusal texts for live queries over HTTP —
   one from the server for a raw `LIVE SELECT`, one from the SDK's `.live()`
   path. Why does the WebSocket negative control in
   `live_queries_are_refused_over_the_http_protocol` first `DEFINE TABLE
   OVERWRITE memory`, and what does the HTTP refusal firing *without* that
   table prove about where the refusal happens?
3. The capability probes classify `Ok`/`Err` but the network probe's target is
   `127.0.0.1:9`. Why can `Ok`/`Err` alone not distinguish "capability denied"
   from "capability granted but unreachable", and how do the recorded verbatim
   texts (server vs embedded) close that gap?
4. The parity suite now asserts `(major, minor, patch) == (3, 2, 4)` at run
   time. What question would a green run against a different server version
   have answered instead, and which §5 run-table row pins the build that was
   actually tested?
5. After this advance, what single residual risk did ADV-STORE-001 lose, and
   which three named constraints replaced it?
