#!/usr/bin/env bash
#
# Totem sync hook body (ADV-CLI-001). Materialized into an enrolled repo's
# .git/hooks/totem-sync-hook by `totem enroll`, and invoked from post-commit
# and post-merge so /arrive/ changes reach the Totem landscape without anyone
# having to remember to run `totem sync` by hand.
#
# Mirrors this repo's own hooks/platform/arrive-platform-pre-commit.sh
# conventions: `set -u` (not `-e`) so a sync failure never blocks a commit or
# merge, silent no-ops when the binary or an /arrive/ tree is absent, and an
# env-var kill switch.
#
# Env:
#   TOTEM_SYNC_HOOK=0   disable entirely
#   TOTEM_BIN           explicit path to the totem binary

set -u

if [ "${TOTEM_SYNC_HOOK:-1}" = "0" ]; then
  exit 0
fi

root="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0
if [ ! -d "$root/arrive" ]; then
  exit 0
fi

bin="${TOTEM_BIN:-}"
if [ -z "$bin" ]; then
  bin="$(command -v totem || true)"
fi
if [ -z "$bin" ]; then
  exit 0
fi

"$bin" sync --path "$root" >/dev/null 2>&1 || true
