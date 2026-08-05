#!/usr/bin/env bash
#
# ARRIVE platform freshness hook (TD-PLAT-012, ADV-HOOKS-005).
# Intended for git post-merge / post-checkout: surfaces stale platform context
# the moment the working tree changes.
#
# Warn-only by design: this hook NEVER blocks the git operation (always exits
# 0). Blocking enforcement belongs to `arrive platform check --strict` in CI.
# No-op when the repo has no `platform:` block (TD-PLAT constraint 1/8).
#
# Env:
#   ARRIVE_PLATFORM_HOOKS=0        disable entirely
#   ARRIVE_PLATFORM_HOOK_NO_FETCH=1  skip the remote lookup (offline machines)
#   ARRIVE_BIN                     explicit path to the arrive binary

set -u

if [ "${ARRIVE_PLATFORM_HOOKS:-1}" = "0" ]; then
  exit 0
fi

root="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0
registry="$root/arrive/registry.yaml"
if [ ! -f "$registry" ] || ! grep -q '^platform:' "$registry"; then
  exit 0
fi

bin="${ARRIVE_BIN:-}"
if [ -z "$bin" ]; then
  bin="$(command -v arrive || true)"
fi
if [ -z "$bin" ]; then
  exit 0
fi

extra_args=""
if [ "${ARRIVE_PLATFORM_HOOK_NO_FETCH:-0}" = "1" ]; then
  extra_args="--no-fetch"
fi

# shellcheck disable=SC2086  # intentional word-splitting of optional flag
if ! (cd "$root" && "$bin" platform check $extra_args); then
  echo "ARRIVE: platform check reported issues (warn-only — see above)." >&2
fi

exit 0
