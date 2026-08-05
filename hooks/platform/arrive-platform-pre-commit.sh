#!/usr/bin/env bash
#
# ARRIVE platform pre-commit hook (TD-PLAT-012, ADV-HOOKS-005).
# Platform-role repos only: refuses commits that would land a stale
# arrive/platform/INDEX.md or inconsistent platform declarations.
#
# Member repos and repos without a `platform:` block are untouched no-ops
# (TD-PLAT constraint 1/8). Bypass: ARRIVE_PLATFORM_HOOKS=0 or --no-verify.
#
# Env:
#   ARRIVE_PLATFORM_HOOKS=0   disable entirely
#   ARRIVE_BIN                explicit path to the arrive binary

set -u

if [ "${ARRIVE_PLATFORM_HOOKS:-1}" = "0" ]; then
  exit 0
fi

root="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 0
registry="$root/arrive/registry.yaml"
if [ ! -f "$registry" ] || ! grep -q '^platform:' "$registry"; then
  exit 0
fi
# Enforce only in platform-role repos.
if ! sed -n '/^platform:/,/^[^ ]/p' "$registry" | grep -q 'role: platform'; then
  exit 0
fi

bin="${ARRIVE_BIN:-}"
if [ -z "$bin" ]; then
  bin="$(command -v arrive || true)"
fi
if [ -z "$bin" ]; then
  exit 0
fi

cd "$root" || exit 0

if ! "$bin" platform index --check; then
  echo "" >&2
  echo "ARRIVE: arrive/platform/INDEX.md is stale." >&2
  echo "  Run 'arrive platform index' and stage the regenerated file." >&2
  exit 1
fi

if ! "$bin" platform doctor --strict; then
  echo "" >&2
  echo "ARRIVE: platform declarations are inconsistent." >&2
  echo "  Run 'arrive platform doctor' for detail and fix before committing." >&2
  exit 1
fi

exit 0
