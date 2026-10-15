#!/usr/bin/env bash
# Linux build-lane entry point. Provision with ctox-prep.sh before invoking.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
exec python3 scripts/sync-v4/gate-critical.py "$@"
