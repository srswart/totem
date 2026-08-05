# Tech Direction: SurrealDB

**Status:** Findings accepted · **Date:** 2026-08-05 · **Advance:** ADV-STORE-004
· **Companion:** [solution-intent.md](../solution-intent.md) §1, §7

Totem's architecture rests on one assumption (Solution Intent §1): *one SurrealQL
round trip assembles graph traversal + vector search + temporal facts into
complete context, and a turn's writes commit in one ACID transaction.* Everything
downstream — the read → think → write turn model, the G2 goal, the schema
ADV-STORE-001 is about to commit to — inherits it. This document records what a
spike actually observed, and the constraints that observation places on the
store.

**Verdict: confirmed, with four version-specific constraints** (TD-002, TD-004,
TD-005, TD-006) that the store must honour to keep it true, and one parity
question that could not be closed in this environment (§5).

## 1. What was run

| | |
|---|---|
| SurrealDB | `3.2.4`, pinned exactly (`=3.2.4`) |
| Engine | embedded `kv-mem`, in-process |
| Toolchain | rustc 1.94.1, edition 2024 |
| Code | `crates/totem-store-spike` — toy schema, seed, five experiments |
| Run by | `cargo test -p totem-store-spike` (6 tests) |

The toy schema is the smallest shape that exercises all four models at once: a
`memory` document table with an HNSW-indexed `embedding`, a `datetime`, a scope
string, a record link to a `component` entity, and a `derived_from` graph
relation between memories.

## 2. The load-bearing findings

### TD-001 — One round trip does assemble all four models. *(confirmed)*

A single `SELECT` returns vector-ranked rows, each carrying a graph traversal
(`->derived_from->memory.body`), a resolved record link (`subject.name`), a
temporal cutoff, and a scope filter. Nothing is assembled client-side; no second
query is needed. The recall statement lives in `RECALL_QUERY`
(`crates/totem-store-spike/src/lib.rs`).

The fallback contemplated in ADV-STORE-004 — two coordinated queries — is **not
needed**, and its cost is not incurred.

### TD-002 — `<|K,EF|>` uses the vector index; `<|K,DIST|>` does not. *(constraint)*

The two knn forms are not interchangeable, and the difference is invisible in the
results:

- `embedding <|3,40|> $probe` → plan operator `KnnScan`, `index: mem_embedding`,
  `ef: 40`. The HNSW index does the work.
- `embedding <|3,COSINE|> $probe` → plan operators `TableScan` → `KnnTopK`. A
  full scan with brute-force top-K, **even though the HNSW index exists**.

Both return correct rows on a five-row table, so a test that only checks results
cannot tell them apart. `crates/totem-store-spike/tests/embedded.rs::scope_predicate_is_pushed_into_the_index_scan`
therefore asserts on `EXPLAIN FULL` output, not just rows. The store should keep
an equivalent plan assertion: it is the only thing that will catch a silent
regression to full scans as the corpus grows.

### TD-003 — Scope and temporal predicates are pushed *into* the index scan. *(confirmed)*

This is the finding that matters most for scope isolation. The observed plan:

```
SelectProject
└── Filter          predicate: scope INSIDE [...] AND created_at > d'2026-06-01T00:00:00Z'
    └── KnnScan     index: mem_embedding, k: 3, ef: 40,
                    predicate: scope INSIDE [...] AND created_at > d'2026-06-01T00:00:00Z'
```

The predicate reaches the `KnnScan` itself. The spike's seed data is arranged to
make the alternative visible: the two foreign-scope records are *closer* to the
probe vector than anything readable, so an engine that truncated to top-K by
distance and filtered afterwards would return an empty or short result. It
returns the correct readable rows instead.

**Consequence for the store:** scope filtering can be expressed as an ordinary
predicate in the recall statement without paying an over-fetch penalty. It must
still be *generated* by the store layer, never by a caller — see §4.

**What this does not prove:** five rows with `ef: 40` cannot distinguish "the
predicate is applied during HNSW traversal" from "the graph is small enough that
everything is visited anyway". Under a selective filter on a large corpus, HNSW
can still under-recall if `ef` is too small. Measuring recall against a realistic
corpus belongs to ADV-STORE-005; until then, the store should treat `ef` as a
tuning parameter with an unmeasured floor.

### TD-004 — A temporal cutoff bound as a string silently filters nothing. *(constraint)*

`created_at > $since` with `$since` bound as a `String` raises no parse error, no
type error, and no warning — and filters nothing. SurrealQL compares values of
different types by type rank, so the comparison is constant-true. Bound as a
`surrealdb::types::Datetime`, the same query filters correctly (and the plan then
shows `created_at > d'2026-06-01T00:00:00Z'`).

This was found the hard way: the first version of the spike's recall assertion
failed with an extra row, not with an error.

**Consequence for the store:** every temporal binding must be a typed
`Datetime`, and the store's query layer should carry a regression test for it.
Any query builder that accepts caller-supplied filter values as strings will
silently widen results — including, potentially, past a retention or TTL
boundary. `crates/totem-store-spike/tests/embedded.rs::a_string_temporal_cutoff_silently_filters_nothing`
pins the behaviour.

### TD-005 — `vector::distance::cosine` is not callable in 3.2.4. *(constraint)*

Despite appearing in the crate's internals, the function registry in 3.2.4
exposes `vector::similarity::cosine` (higher is closer) but **not**
`vector::distance::cosine`; calling it fails with `Parse error: Invalid
function/constant path`. `vector::distance::knn()` returns the distance computed
by the knn operator and is only meaningful in a statement that uses one.

**Consequence for the store:** a brute-force ranking fallback must be written
with `vector::similarity::cosine ... ORDER BY ... DESC`, not with a distance
function.

### TD-006 — Transactions are atomic across document, graph, and entity writes. *(confirmed)*

A turn's three writes — `CREATE` a decision, `UPDATE` an entity counter, `CREATE`
a triggered event — commit together inside `BEGIN`/`COMMIT`. When the last write
violates the schema, SurrealDB aborts the whole transaction:

```
statement 2: Couldn't coerce value for field `kind` of `event:bad`: Expected `string` but found `12345`
statement 1: The query was not executed due to a failed transaction
statement 3: Cannot COMMIT: the transaction was aborted due to a prior error
```

Neither the earlier `CREATE` nor the earlier `UPDATE` survives. Verified by
asserting both the absence of the created record and the unchanged counter, and
by a negative control: making the failing statement valid makes the test fail.

### TD-007 — Live queries work on the embedded engine, and only for committed writes. *(confirmed)*

`LIVE SELECT ... WHERE scope = ...` delivers notifications on the embedded
`kv-mem` engine — default `Capabilities` enable live-query notifications, so no
special construction is needed. The rolled-back turn of TD-006 emits nothing.

Proving that absence without a flaky sleep took a second attempt: a drain-until-
quiet loop passed alone but failed under concurrent test load. The assertion now
commits a **sentinel** record after the aborted turn and reads the feed until the
sentinel arrives — anything the aborted turn emitted would have to appear before
it. Deterministic under load, and the same technique is worth reusing wherever
the console asserts on live feeds.

### TD-008 — Live notifications are not ordered by statement within a transaction. *(constraint)*

The committed turn creates `memory:decision` and then updates `memory:mine_rule`.
The feed delivers both, but **not reliably in that order**: repeated runs of the
same test produced `[Create, Update]` and `[Update, Create]` on the same engine
with the same data, roughly one run in four. Only the sentinel — a separate,
later commit — holds a guaranteed position.

This surfaced as a flaky assertion, and the first instinct (widen a timeout) would
have hidden it. The spike's assertion is now deliberately order-insensitive
within the transaction and order-sensitive only across commits.

**Consequence for the console and any curator that consumes the feed:** do not
reconstruct causal order from notification arrival order. Order by a field the
records carry (`created_at`, or an episodic sequence), or treat a transaction's
notifications as an unordered set. A "latest write wins" reducer driven by feed
order will be wrong roughly a quarter of the time on multi-write turns.

## 3. Cost note

A cold build of the spike crate (dev profile, `kv-mem` only, no RocksDB) took
**5m02s** on the project's cloud runner. SurrealDB dominates the workspace's
compile time. Keep on-disk engines behind an optional feature, and expect this
cost on every cold ephemeral runner from ADV-STORE-001 onward.

## 4. Constraints handed to ADV-STORE-001

1. Pin `surrealdb = "=3.2.4"`; every finding above is version-specific.
2. Default features `kv-mem` only; RocksDB behind an optional cargo feature.
3. Use the `<|K,EF|>` knn form, and assert on `EXPLAIN FULL` — results alone
   cannot prove the index is used (TD-002).
4. Express scope as a predicate in the recall statement; it is pushed into the
   index scan and costs no over-fetch (TD-003). Generate it in the store layer.
   The spike deliberately proves only that SurrealDB *can* filter correctly — the
   invariant that it *always* does is the store's to establish and test.
5. Bind datetimes as `Datetime`, never as strings, and carry a regression test
   (TD-004).
6. Live-consuming surfaces (console feed) must use the embedded engine or the
   WebSocket protocol, not HTTP (§5).
7. If a future SurrealDB release stops pushing predicates into `KnnScan`, the
   fallback is to over-fetch (`K' >> K`) and re-filter in the same statement,
   with a documented recall budget — not to filter above the store.

## 5. Engine parity — partially closed

The advance asked whether every capability behaves the same on the embedded
engine (what all tests use) as on server mode (what production will use).

**Verified by execution:** nothing on the server. **No `surreal` server could be
run in this environment** — the agent sandbox's egress proxy refuses
`download.surrealdb.com` and `install.surrealdb.com` (`CONNECT tunnel failed,
response 403`), and the GitHub releases API is not enabled for that repository in
this session. Building the server from source was not attempted; it is far larger
than the 5-minute client build.

**Established from the SDK source, not executed:**

- Live queries are **not supported over the HTTP remote protocol** — the HTTP
  engine returns *"The protocol or storage engine does not support live queries
  on this architecture"*. Embedded and WebSocket only. This is a real constraint
  on the gateway/console design, not a nuance.
- `engine::local` executes queries through `surrealdb_core::kvs::Datastore` —
  the same datastore `surreal start` wraps. The query engine, planner, and
  transaction semantics are shared code; what differs between embedded and
  server is transport, authentication, and capability defaults (an embedded
  instance runs with default `Capabilities` and no auth; a server applies its own
  flags). So parity of *query semantics* is architecturally expected, while
  parity of *auth and capability behaviour* is exactly what a server run would
  need to check — and is most relevant to ADV-GATEWAY-003.

**How to close it:** `crates/totem-store-spike/tests/server_parity.rs` re-runs
every experiment above over WebSocket against a real server. It is behind the
`server-parity` feature and skips unless `TOTEM_SPIKE_SURREAL_URL` is set, so it
can never hang a sandboxed run. It compiles, but **has never been executed
against a server**. On any machine with the binary:

```sh
surreal start --user root --pass root memory &
TOTEM_SPIKE_SURREAL_URL=ws://127.0.0.1:8000 \
  cargo test -p totem-store-spike --features server-parity --test server_parity
```

Until someone runs that, treat server-mode parity as *expected but unverified*,
and carry it as a residual risk on ADV-STORE-001.

## 6. What this spike deliberately did not answer

- Recall quality of HNSW under selective scope filters at realistic corpus size
  (→ ADV-STORE-005's evaluation corpus).
- Embedding dimensions, provider, and where embedding happens (→ ADV-STORE-003).
- Any performance number. The spike's timings come from a five-row table and are
  not a workload model; nothing here is performance evidence.
