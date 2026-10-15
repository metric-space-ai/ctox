#!/usr/bin/env bash
# Controller entry: exit 2 = prerequisites absent/not_run; never a green empty gate.
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
TARGET=v3
args=("$@")
for ((i=0;i<${#args[@]};i++)); do
  if [[ "${args[$i]}" == --target ]]; then TARGET="${args[$((i+1))]:-}"; fi
done
if [[ "$TARGET" == v4=* ]]; then
  GPU_HOST="${E2E_GPU_HOST:-auto}"
  kernel_set=false
  for arg in "${args[@]}"; do [[ "$arg" != --kernel-revision ]] || kernel_set=true; done
  if [[ "$kernel_set" == false ]]; then
    kernel=$(git -C "$REPO" rev-parse --verify --end-of-options 'refs/remotes/origin/v4/k-1b-shell-api^{commit}' 2>/dev/null || true)
    [[ "$kernel" =~ ^[a-f0-9]{40}$ ]] && args+=(--kernel-revision "$kernel")
  fi
  ref="${TARGET#v4=}"
  # Pin a branch on the controller. A remote lane mirror may have an older ref.
  if [[ "$ref" =~ ^[A-Za-z0-9._/-]+$ ]]; then
    revision=$(git -C "$REPO" rev-parse --verify --end-of-options "${ref}^{commit}" 2>/dev/null || git -C "$REPO" rev-parse --verify --end-of-options "refs/remotes/origin/${ref}^{commit}" 2>/dev/null || true)
    if [[ "$revision" =~ ^[a-f0-9]{40}$ ]]; then
      for ((i=0;i<${#args[@]};i++)); do
        if [[ "${args[$i]}" == --target ]]; then args[$((i+1))]="v4=$revision"; fi
      done
      args+=(--target-requested "$ref")
    fi
  fi
else GPU_HOST="${E2E_GPU_HOST:-gpu4}"; fi
cd "$REPO"
exec greppy bash-smart -e 'K[0-9]+ \||e2e-k-|ZERO|Run ID|receipt' -- \
  node tests/sync-v4/k-support/lane-controller.mjs -- "$HOME/.codex/bin/gpu-build-run.sh" --owner 01a087a0-169a --task v4-e2e-harness \
  --target-key v4-v4-e2e-harness --host "$GPU_HOST" --jobs 2 --src "${E2E_SOURCE_STAGE:-$REPO}" \
  -- bash -lc 'node scripts/sync-v4/e2e-check.mjs && node tests/sync-v4/k-support/gate.mjs "$@"' e2e-k "${args[@]}"
