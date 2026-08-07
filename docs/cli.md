# The `totem` CLI

What exists today. The CLI is young — two commands — and this document grows
with it. Where something does not work yet, it says so rather than implying
coverage.

Build it with `cargo build -p totem-cli`; the binary is `target/debug/totem`.

## Credentials come first

Every call to a gateway needs a bearer credential. The CLI resolves one in a
fixed order and **fails rather than proceeding anonymously** — a CLI that
quietly skipped authentication would work against a loopback gateway and fail
confusingly against a real one.

1. `--token <value>` on the command line
2. `TOTEM_TOKEN` in the environment
3. A credential in `~/.totem/credentials.json` **bound to the repo being
   addressed** — one issued for a different repo is refused locally rather
   than presented, because the gateway would answer 403 and that is a worse
   error than "you have no credential for this repo"

With none of them you get a message naming all three, not a bare 401:

```
Error: no credential for repo srswart/totem. Supply one with --token, or set
TOTEM_TOKEN, or add one for this repo to /Users/you/.totem/credentials.json
```

On a new machine, the deployment's bootstrap token is the first credential —
`fly secrets` holds it for the deployed gateway.

## `totem enroll`

Parse this repo's `arrive/` tree, send the landscape snapshot to a gateway,
and (unless told not to) install a `post-commit` hook that keeps it fresh.

```sh
totem enroll --gateway-url https://totem-dev.fly.dev --token "$TOKEN"

# against a local gateway, without touching git hooks
totem enroll --gateway-url http://127.0.0.1:8787 --no-hook --token "$TOKEN"
```

| Flag | Meaning |
|---|---|
| `--gateway-url` | Required. Base URL, no trailing `/enroll`. |
| `--token` | Bearer credential; see resolution order above. |
| `--repo` | The `owner/name` id used to pick a stored credential. Defaults to the identity the landscape names, so you rarely need it. |
| `--repo-root` | Repo root containing `arrive/`. Defaults to `.`. |
| `--source` | Provenance recorded for the sync run. Defaults to `cli:enroll`. |
| `--no-hook` | Skip installing the post-commit hook. |

Output on success names where the credential came from, so a surprising
identity is traceable without ever printing the token:

```
authenticating as the credential from --token
synced 1 system(s), 8 component(s), 44 advance(s)
installed sync hook at ./.git/hooks/post-commit
```

### The sync hook

`enroll` installs `.git/hooks/post-commit`, which re-runs `totem enroll` after
each commit so the landscape tracks the repo. **It contains no credential**:
it resolves one at run time from `TOTEM_TOKEN` or the local store. A token
written into a hook would sit in `.git/hooks`, survive in backups, and be read
by anything with filesystem access — and the hook runs unattended, which is
when a leak goes unnoticed.

**Not yet exercised:** the hook has not been run unattended against the
deployed gateway, because that needs a credential in the *store* and the
bootstrap token currently lives in a file. It becomes testable when the
gateway's credential-issuance endpoints land (ADV-GATEWAY-012 deferred them to
their consumer).

## `totem credential create`

Issue a credential bound to exactly one repo, scope, and actor, and write it
to `~/.totem/credentials.json` (mode `0600`).

```sh
totem credential create --repo srswart/totem \
  --scope project:srswart/totem --actor ada
```

**Read this before relying on it.** Today this mints a credential *record*
locally — the shape of a least-privilege credential — but **no gateway
verifies it**. Server-side issuance and verification are separate work
(ADV-GATEWAY-012 built the durable registry; its issue/list/revoke endpoints
are deferred to the advance that consumes them). A credential created here is
useful for exercising the local store, not for authenticating to a deployed
gateway. Use the bootstrap token for that.

Scope forms: `actor:<id>`, `project:<owner>/<name>`, `team:<id>`, `platform`.
A scope must belong to the repo and actor it is issued for; an over-broad
binding is refused at creation.

## Not in the CLI yet

Named so nobody hunts for them:

- **Recall and save.** There is no `totem recall` or `totem save`. Memory
  reads and writes go through the MCP tools (agents) or the REST API (`curl`).
- **Credential list / revoke.** `create` is the only credential subcommand.
- **Gateway configuration.** `--gateway-url` is passed per call; nothing is
  persisted, so the hook carries the URL it was installed with.

## Related

- `.claude/skills/totem-memory/SKILL.md` — how to *use* memory once connected
- `infra/RUNBOOK.md` — operating the deployed gateway
- `docs/dogfood/plan.md` — where the CLI fits in the trial
