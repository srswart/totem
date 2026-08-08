# Totem on Fly.io — runbook

App `totem-dev`, region `sin`, public endpoint `https://totem-dev.fly.dev`
(ADV-INFRA-002, implementing DEP-001).

## ⚠️ Exactly one machine, always

The gateway owns an embedded RocksDB store with an **exclusive lock**
(DEP-001). A second machine cannot open it. Every Fly default fights this —
two machines on deploy, rolling updates, auto-start — so `fly.toml` pins one
machine, `strategy = "immediate"` (stop-then-start), and disables auto-stop.

**Never** `fly scale count 2`, never switch to a rolling strategy, never
enable autoscaling. The failure is loud (a machine that cannot start), not
silent — but it is downtime.

Verify after any change to those settings:

```sh
fly deploy -a totem-dev --ha=false     # twice in a row
fly machines list -a totem-dev          # exactly one, state=started, checks 1/1
```

## Deploy

```sh
fly deploy -a totem-dev --ha=false
```

**Build times, and what to expect (ADV-INFRA-006).** Dependency compilation
is a cached layer, so what a deploy costs depends on *what changed*:

| what changed | expect |
|---|---|
| existing source only | ~3–5 min — the dependency layer is reused |
| a **new** test/bench/example/bin *file* | ~35 min — full rebuild |
| any `Cargo.toml` or `Cargo.lock` | ~35 min — full rebuild |

The middle row is the surprising one and it is not a bug. Cargo
auto-discovers `tests/*.rs` as targets, and `cargo-chef` bakes the resolved
manifest — auto-discovered targets included — into `recipe.json`. So adding
one test file changes the recipe and rebuilds every dependency, with no
dependency having changed. Measured on deploy 2 (ADV-GATEWAY-016): 1986s of
`cook` for a single new test file.

Expect it rather than debug it. A deploy that adds a test file is a long
deploy.

**If a source-only deploy still takes ten minutes, the cache has silently
stopped working.** The usual cause is the Dockerfile's `cargo chef cook` and
`cargo build` lines drifting apart — different features, package, or profile.
Docker still reuses the cooked layer, then cargo rebuilds every dependency
anyway because the fingerprints differ, so it costs full price while
reporting `CACHED`. The two lines are deliberately adjacent in the
Dockerfile; keep them that way.

A deploy after a dependency change is *supposed* to be slow. That is the
cache busting correctly, not a regression.

`.dockerignore` is load-bearing: without it the build context includes
`target/` (~49 GB) and the 128 MB model cache, and the deploy appears to
hang for hours while uploading.

## Currency figures will fall after ADV-GATEWAY-017

The console used to browse by calling `/recall`, which reinforces everything
it returns — so every record you looked at had its `currency` reset to 1.0 and
its `use_count` incremented. It now browses through `/recall/explain`, which
does not write.

**Expect currency on the dogfood estate to decay from here.** That is the
correct behaviour finally showing, not a regression: those figures were being
propped up by us looking at them.

The inflation already recorded cannot be undone — nothing distinguishes a
reinforcement that came from browsing from one that came from an agent using
a memory. It is an argument for calibrating against a purpose-built corpus
(ADV-STORE-009) rather than trying to rehabilitate this estate.

## Secrets and configuration

Non-secret configuration lives in `fly.toml [env]`. Secrets are set with:

```sh
fly secrets set --stage NAME=value -a totem-dev   # then `fly deploy`
fly secrets list -a totem-dev
```

**Always `--stage` when the new secret needs config that ships with the same
deploy.** The gateway exits(1) on a *partially* configured bootstrap
credential, so setting `TOTEM_BOOTSTRAP_TOKEN` alone against a running image
that lacks the other three variables crash-loops the machine.

Current secrets: `TOTEM_BOOTSTRAP_TOKEN` (the bearer credential; the local
copy lives at `~/.totem/bootstrap-token`, mode 0600 — move it to a password
manager).

## Backup and restore

Fly takes **scheduled daily snapshots** with 5-day retention (volume created
with them enabled). Take one manually before anything risky:

```sh
fly volumes list -a totem-dev
fly volumes snapshots create <volume-id>
fly volumes snapshots list <volume-id>
```

Restore creates a *new* volume from a snapshot, which the app must then be
pointed at:

```sh
fly volumes create totem_data -a totem-dev -r sin -n 1 --snapshot-id <snap-id>
```

**Not yet exercised end to end.** A snapshot was created and listed
(`vs_3ZL1GoXKxeKsPPz37g7p`), but a restore has never been performed — see the
advance's Evidence section, where this is recorded as unverified rather than
claimed. Verify it before the memory estate matters.

## Changing the embedding model (ADV-STORE-008)

Recall ranks by cosine distance. Vectors from two different models do not
share a geometry, so an index holding both keeps returning results — in a
confident order — that means nothing. There is no error and no symptom until
somebody trusts a ranking. **Never leave the index mixed.**

Ask what is in there:

```sh
curl -sH "Authorization: Bearer $TOKEN" https://totem-dev.fly.dev/admin/embedding
```

`uniform: true` and a single entry in `rows_by_model` is the healthy state.
`(unlabelled)` is a row written before schema v11; the pass treats it as
stale, which is correct — its space is genuinely unknown.

To move the estate into a new model's space:

1. **Snapshot first.** Re-embedding rewrites every vector; the pass is
   resumable but not reversible.
   ```sh
   fly volumes snapshots create $(fly volumes list -a totem-dev --json | jq -r '.[0].id')
   ```
2. Deploy the image built with the new model.
3. Run the pass. It only touches rows outside the running model's space, so
   re-running after an interruption resumes rather than redoing:
   ```sh
   curl -sX POST -H "Authorization: Bearer $TOKEN" https://totem-dev.fly.dev/admin/reembed
   ```
4. Confirm `uniform: true`, then run the EMB-004 golden queries. The
   paraphrase query is the one that matters: it is the query a lexical
   baseline fails, so it is the only cheap check that the *real* model is in
   the path rather than a stub that loaded successfully.

The pass runs inside the gateway process because DEP-001 makes it the store's
sole owner — there is no separate one-shot binary to run, and there cannot
be while the gateway holds the engine lock. It is deliberately not a start-up
migration: at boot it would hold the health check open for its whole duration
on a machine count of one, and re-run on every restart with nobody deciding
it should.

**If the gateway will not start**, a build with `--features fastembed` that
cannot load its model panics on purpose rather than falling back to the
non-semantic stub. Check that `/models` is present in the image and
`FASTEMBED_CACHE_DIR` points at it. A deployment that silently downgraded
would report healthy while serving meaningless rankings.

## Health and logs

```sh
curl https://totem-dev.fly.dev/health      # "ok", unauthenticated by design
fly logs -a totem-dev
fly status -a totem-dev
```

`/health` is deliberately outside the auth layer (Fly's checker cannot present
a credential). It is the only such route besides the OAuth discovery documents
that ADV-GATEWAY-013 will add; the list lives in one place,
`totem_gateway::unauthenticated_routes`.

## Authenticated calls

```sh
TOKEN=$(cat ~/.totem/bootstrap-token)
curl -s -X POST https://totem-dev.fly.dev/recall \
  -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"actor":"shawn","project":"srswart/totem","teams":[],"query":null,
       "categories":[],"since":null,"limit":null,"harness":"claude_code",
       "session":"manual","turn":null}'
```

The CLI cannot do this yet — it sends no credential (ADV-CLI-002). Until then,
enrollment is a manual `POST /enroll` with the flattened snapshot body.
