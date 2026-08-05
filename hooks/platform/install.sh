#!/usr/bin/env bash
#
# Install ARRIVE platform git hooks into .git/hooks (ADV-HOOKS-005).
# Idempotent: creates hook files if absent, appends a marked invocation line
# to existing hooks, and never duplicates it.
#
# Usage: ./hooks/platform/install.sh   (run from anywhere inside the repo)

set -euo pipefail

root="$(git rev-parse --show-toplevel)"
hooks_src="$root/hooks/platform"
hooks_dst="$root/.git/hooks"
marker="# arrive-platform-hook"

install_line() {
  local hook_name="$1"
  local script_rel="$2"
  local dst="$hooks_dst/$hook_name"
  local line="\"\$(git rev-parse --show-toplevel)\"/$script_rel $marker"

  if [ ! -f "$dst" ]; then
    printf '#!/usr/bin/env bash\n%s\n' "$line" > "$dst"
    chmod +x "$dst"
    echo "created .git/hooks/$hook_name"
    return
  fi

  if grep -qF "$marker" "$dst"; then
    echo "up to date: .git/hooks/$hook_name"
    return
  fi

  printf '\n%s\n' "$line" >> "$dst"
  chmod +x "$dst"
  echo "appended to .git/hooks/$hook_name"
}

chmod +x "$hooks_src/arrive-platform-freshness.sh" "$hooks_src/arrive-platform-pre-commit.sh"

install_line "post-merge" "hooks/platform/arrive-platform-freshness.sh"
install_line "post-checkout" "hooks/platform/arrive-platform-freshness.sh"
install_line "pre-commit" "hooks/platform/arrive-platform-pre-commit.sh"

echo "Done. Disable at any time with ARRIVE_PLATFORM_HOOKS=0."
