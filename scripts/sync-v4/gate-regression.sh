#!/usr/bin/env bash
# Entrypoint on the controller. Linux acceptance always goes through the shared lane.
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
GPU_HOST="${E2E_GPU_HOST:-gpu4}"
cd "$REPO"
exec greppy bash-smart -e 'K[0-9]+ \||e2e-k-|ZERO|Run ID|receipt' --  "$HOME/.codex/bin/gpu-build-run.sh" --owner 01a087a0-169a --task v4-e2e-harness  --target-key v4-v4-e2e-harness --host "$GPU_HOST" --jobs 2 --src "${E2E_SOURCE_STAGE:-$REPO}"  -- bash -lc 'node scripts/sync-v4/e2e-check.mjs && node scripts/sync-v4/e2e-gate.mjs "$@"' e2e-k "$@"
