#!/usr/bin/env bash
#
# Install the vendored ARRIVE CLI into a Linux cloud sandbox.
#
# The repo vendors arrive-cli-*-linux-*.tar.gz under arrive-linux/ so CI and
# scheduled cloud agents get a pinned CLI without a network fetch. This script
# installs it user-local (no root) and refuses to continue if the environment
# cannot actually run it — an ungoverned implementation run is worse than a
# failed one.
#
# Usage:  ./scripts/bootstrap-arrive.sh
# Then:   export PATH="$HOME/.local/bin:$PATH"
#
# Exits non-zero on any preflight failure.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="$REPO_ROOT/arrive-linux"
EXTRACT_DIR="${TMPDIR:-/tmp}/arrive-cli"
INSTALL_BIN="$HOME/.local/bin/arrive"

fail() {
  echo "ERROR: $*" >&2
  echo >&2
  echo "Refusing to continue. ARRIVE governance commands (doctor, check," >&2
  echo "template render, plan) are required for this repo — proceeding" >&2
  echo "without them would produce unreviewable, ungoverned changes." >&2
  exit 1
}

# ── Preflight: architecture ───────────────────────────────────────────────────
# Select the tarball that matches this host rather than assuming x86-64. An
# arch mismatch fails at exec time with "Exec format error", which is easy to
# misread as a corrupt download, so resolve it here with a clear message.
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64 | amd64)   ARRIVE_ARCH="x86_64"  ;;
  aarch64 | arm64)  ARRIVE_ARCH="aarch64" ;;
  *) fail "unsupported architecture '$ARCH'; ARRIVE ships linux x86_64 and aarch64 builds." ;;
esac
echo "Architecture: $ARCH (expecting arrive-cli-*-linux-${ARRIVE_ARCH}.tar.gz)"

# ── Preflight: libc ───────────────────────────────────────────────────────────
# The binary is dynamically linked against glibc. musl-based images (Alpine)
# cannot load it.
if command -v ldd >/dev/null 2>&1; then
  if ldd --version 2>&1 | head -1 | grep -qi musl; then
    fail "musl libc detected; the vendored CLI requires glibc."
  fi
fi

# ── Locate the tarball ────────────────────────────────────────────────────────
# Glob rather than hard-code the version so a version bump does not silently
# break the loop.
shopt -s nullglob
TARBALLS=("$VENDOR_DIR"/arrive-cli-*-linux-"$ARRIVE_ARCH".tar.gz)
OTHER_ARCH=("$VENDOR_DIR"/arrive-cli-*-linux-*.tar.gz)
shopt -u nullglob

if [[ ${#TARBALLS[@]} -eq 0 ]]; then
  msg="no arrive-cli-*-linux-${ARRIVE_ARCH}.tar.gz found at $VENDOR_DIR"
  if [[ ${#OTHER_ARCH[@]} -gt 0 ]]; then
    # The common case: the repo vendors one arch and the host is the other.
    # Name the exact file to add so the fix is a single drop-in commit.
    have="$(basename "${OTHER_ARCH[0]}")"
    ver="${have#arrive-cli-}"; ver="${ver%%-linux-*}"
    msg="$msg
       Present but wrong architecture: $have
       This host is $ARCH. Vendor the matching build and commit it next to
       the existing tarball — both architectures can coexist:
         arrive-cli-${ver}-linux-${ARRIVE_ARCH}.tar.gz"
  fi
  fail "$msg"
fi
if [[ ${#TARBALLS[@]} -gt 1 ]]; then
  fail "multiple ARRIVE tarballs found for ${ARRIVE_ARCH}; expected exactly one:
       ${TARBALLS[*]}"
fi

TARBALL="${TARBALLS[0]}"
echo "Tarball: $(basename "$TARBALL")"

# ── Install ───────────────────────────────────────────────────────────────────
rm -rf "$EXTRACT_DIR"
mkdir -p "$EXTRACT_DIR"
tar -xzf "$TARBALL" -C "$EXTRACT_DIR"

[[ -x "$EXTRACT_DIR/install-from-tarball.sh" ]] \
  || fail "install-from-tarball.sh missing or not executable in $TARBALL"

# The bundled installer places the binary in ~/.local/bin AND seeds
# ~/.arrive/templates. The templates are not optional: template-first authoring
# (`arrive template render`) is how advances get valid frontmatter, and the CLI
# resolves templates from that path. Copying just the binary would leave
# `template render` broken in a way that only surfaces mid-run.
"$EXTRACT_DIR/install-from-tarball.sh"

# ── Verify ────────────────────────────────────────────────────────────────────
[[ -x "$INSTALL_BIN" ]] || fail "expected an executable at $INSTALL_BIN after install"

if ! VERSION_OUT="$("$INSTALL_BIN" --version 2>&1)"; then
  fail "installed binary will not run:
       $VERSION_OUT
       On a glibc host this usually means a missing shared library (libdbus)."
fi
echo "Installed: $VERSION_OUT"

TEMPLATES_DIR="${ARRIVE_CONFIG_DIR:-$HOME/.arrive}/templates"
[[ -d "$TEMPLATES_DIR" ]] \
  || fail "templates not installed at $TEMPLATES_DIR; 'arrive template render' would fail"

# Prove template rendering actually works rather than assuming the directory
# being present is sufficient.
"$INSTALL_BIN" -C "$REPO_ROOT" template render --kind advance --json >/dev/null \
  || fail "'arrive template render' failed; templates are present but unusable"

# Prove the repo's governance artifacts parse with this CLI build.
"$INSTALL_BIN" -C "$REPO_ROOT" doctor artifacts >/dev/null \
  || fail "'arrive doctor artifacts' failed against $REPO_ROOT"

echo
echo "ARRIVE bootstrap OK."
echo "Add to PATH for this shell:  export PATH=\"\$HOME/.local/bin:\$PATH\""
