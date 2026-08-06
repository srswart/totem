# Tech Direction: Deployment topology

**Status:** Decided (DEP-001) · **Date:** 2026-08-06 · **Advances:**
ADV-STORE-006 (evidence), ADV-INFRA-001 (implements) · **Companion:**
[solution-intent.md](../solution-intent.md) §9,
[surrealdb.md](surrealdb.md) §5 (TD-009..TD-011)

Solution Intent §9 left the deployment topology open with a lean: "one shared
Totem per team… start shared-single-instance; revisit if offline use matters."
This record pins the shape of that shared single instance, decided by Shawn on
2026-08-06 from the executed ADV-STORE-006 findings.

## DEP-001 — The shared instance is the gateway with an embedded RocksDB store

**Decision:** `totem-gateway` is the one long-running Totem process. It opens
an on-disk embedded SurrealDB engine (RocksDB, via the store's existing
optional cargo feature) at a configured data directory. Every other process —
`totem` CLI, the console, MCP surfaces, future curators — is a **client of the
gateway**, never of the database. There is no separate database service.

**Why, from the evidence:**

- **TD-011 removed option B's only benefit.** A separate `surreal start`
  server would let other processes connect directly — but a least-privilege
  DB user's writes are *silently discarded* (no error, nothing persisted), so
  DB roles cannot make direct connections safe, and every direct connection
  is an unlogged access path around the gateway's audit log. Under embedded
  RocksDB the single-owner rule is not a convention but a physical property:
  the engine's lock prevents any second process from opening the data
  directory while the gateway runs. The "no unlogged access" invariant
  becomes architecture.
- **TD-009 is a non-issue embedded.** Live queries work natively in-process;
  the CONSOLE-003 event relay subscribes inside the gateway either way.
- **TD-010's surface vanishes.** No server capability flags to configure, no
  DB credentials to issue, rotate, or leak.
- **Ops floor is minimal.** One process, one data directory to back up. No
  second service to version-pin, monitor, or secure.

**The accepted cost, and its insurance:** embedded storage cannot scale out
to multiple gateway instances. Accepted deliberately — §9's shared
single instance needs exactly one. When multi-instance day comes, the switch
is to server mode over WebSocket, and **ADV-STORE-006 already executed that
parity check**: all store behaviour is identical against a real server
(`=3.2.4`), no isolation-relevant divergence. The migration risk this
decision defers is a risk that has already been retired.

**Bounds:**

- The server-parity harness (`totem-store-spike`, `server-parity` feature)
  stays maintained as the standing insurance policy; re-run it before any
  future engine switch and on SurrealDB version bumps.
- Per-developer offline instances (§9's "revisit if offline use matters")
  remain out of scope until someone actually needs one.
- Console human authentication stays a separate open question (§9; likely
  reverse-proxy/SSO in front of the gateway).

**Implemented by:** ADV-INFRA-001 (durable store wiring, data directory
configuration, backup/restore, and re-pointing the CLI/MCP islands at the
gateway). Until it lands, every process remains per-run in-memory — the
demo-mode behaviour observed 2026-08-06.
