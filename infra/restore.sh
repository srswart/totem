#!/usr/bin/env bash
# Restore a Totem data directory from an offline snapshot (DEP-001).
#
# The gateway must NOT be running. The current data directory, if present,
# is moved aside (never deleted — Totem is the audit substrate, and restore
# must not be the operation that destroys the only other copy), then the
# snapshot is copied into place.
#
# Usage: infra/restore.sh <snapshot-dir> <data-dir>
set -euo pipefail

snapshot="${1:?usage: restore.sh <snapshot-dir> <data-dir>}"
data_dir="${2:?usage: restore.sh <snapshot-dir> <data-dir>}"

if [ ! -d "$snapshot" ]; then
  echo "no such snapshot: $snapshot" >&2
  exit 1
fi

if [ -d "$data_dir" ]; then
  aside="$data_dir.pre-restore-$(date -u +%Y%m%dT%H%M%SZ)"
  mv "$data_dir" "$aside"
  echo "existing data dir moved aside to $aside" >&2
fi

mkdir -p "$data_dir"
cp -a "$snapshot/." "$data_dir/"
echo "$data_dir"
