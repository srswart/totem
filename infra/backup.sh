#!/usr/bin/env bash
# Offline snapshot backup of the Totem data directory (DEP-001).
#
# The gateway must NOT be running: the snapshot copies a closed data
# directory, and the engine lock guarantees that if this copy succeeds
# nothing was mid-write. Stop the gateway, back up, restart — the
# single-owner topology makes stop-copy-start trivially correct, which is
# why v1 backup is a file copy and not an online export (recorded in
# ADV-INFRA-001; online export can come with a later infra advance).
#
# Usage: infra/backup.sh <data-dir> <backup-root>
# Creates <backup-root>/totem-<UTC timestamp>/ and prints its path.
set -euo pipefail

data_dir="${1:?usage: backup.sh <data-dir> <backup-root>}"
backup_root="${2:?usage: backup.sh <data-dir> <backup-root>}"

if [ -f "$data_dir/LOCK" ] && ! (set -o noclobber; : >>"$data_dir/LOCK") 2>/dev/null; then
  echo "refusing: $data_dir looks locked — is the gateway still running?" >&2
  exit 1
fi

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
target="$backup_root/totem-$stamp"
mkdir -p "$target"
cp -a "$data_dir/." "$target/"
echo "$target"
