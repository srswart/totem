---
advance:
  id: "ADV-STORE-006"
  title: "Investigation: SurrealDB server-mode parity (embedded vs `surreal start`)"
  system: "058-totem-core"
  primary_component: "store"
  components: ["store"]
  started_at: "2026-08-05T08:59:40Z"
  implementation_completed_at: ~
  review_time_estimate_minutes: 30
  review_time_actual_minutes: ~
  pr_links: []
  external_refs: []
  reviewability_score: 0
  risk_flags: ["auth"]
  evidence: []
  model_usage: []
  schema_version: 2
  mode: investigation
  facets: [software, security]
  work_products: [test_automation]
  status: planned
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

After this advance:
- The parity test has actually executed against a running server, and the result
  is recorded verbatim — either parity confirmed, or each divergence written up
  as a new TD entry.
- §5 of the findings document no longer reads "expected but unverified": it
  states what was executed, against which server version, with which capability
  flags and auth configuration.
- The auth/capability surface is characterised — what a default `surreal start`
  permits versus an embedded instance running with default `Capabilities` and no
  auth — and which of those differences ADV-GATEWAY-003 must account for.
- The residual risk recorded on ADV-STORE-001 is either retired or replaced by a
  specific, named constraint.

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

- [ ] branch: create or confirm feature branch for this advance
- [ ] run the existing parity test against `surreal start` 3.2.4 and record the
      outcome verbatim, including server version and capability flags
- [ ] extend `server_parity.rs` with the surface only a server has: authenticated
      signin, behaviour under a least-privilege (namespace/database-scoped) user
      rather than root, and capability defaults (functions, network, scripting)
- [ ] execute the HTTP-protocol live-query limitation that ADV-STORE-004
      established from SDK source but never ran
- [ ] prove sensitivity: a negative control for each new assertion
- [ ] update `docs/tech-direction/surrealdb.md` §5 and add TD entries for any
      divergence found
- [ ] state plainly whether the ADV-STORE-001 residual risk is retired

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

- [ ] profile:selected-practices — investigation mode, `test_automation` work
      product: `automation_control_validation` applies; `tdd` and `tidy_first`
      do not. `tdd:red-green` must not be claimed.
- [ ] investigation:findings — new/updated TD entries in
      `docs/tech-direction/surrealdb.md`, each tied to an executed run.
- [ ] tests:integration — the parity test executed against a real server, with
      server version and capability flags recorded alongside the result.
- [ ] automation:control-validation — negative control per new assertion,
      positive control from the six existing experiments.

## CI Evidence Notes

- CI cannot produce this evidence today: the workflow runner has no SurrealDB
  server. Do not claim `ci:passed` for the parity result. If a service container
  is added to the workflow later, record it then — and only then.
- Externally-run checks before merge: `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `arrive pr check --strict --json`, `arrive doctor artifacts`,
  `arrive plan check`, `arrive check --strict`, `arrive score`.

## Changes Made

- None yet
