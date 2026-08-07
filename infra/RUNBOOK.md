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

Builds take ~10–15 minutes: nothing caches between deploys yet (`COPY . .`
invalidates on any source change, so SurrealDB and RocksDB recompile every
time). A cargo-chef layered Dockerfile would fix this — recorded as a known
cost, not a mystery.

`.dockerignore` is load-bearing: without it the build context includes
`target/` (~49 GB) and the 128 MB model cache, and the deploy appears to
hang for hours while uploading.

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
