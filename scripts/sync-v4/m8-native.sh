#!/usr/bin/env bash
# Execute only inside the canonical Linux build lane, with isolated disposable output.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ "$(uname -s)" != Linux || -z "${CARGO_TARGET_DIR:-}" ]]; then
  echo "m8-native requires the canonical Linux lane and its CARGO_TARGET_DIR" >&2
  exit 64
fi
if [[ "${1:-}" == --qualify-only ]]; then
  shift
  exec python3 "$root/scripts/sync-v4/m8/qualify.py" "$@"
fi
exec python3 "$root/scripts/sync-v4/m8/run.py" "$@"
