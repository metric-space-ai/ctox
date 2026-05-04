#!/usr/bin/env bash
set -euo pipefail

tokens="${1:-4096}"
iterations="${2:-3}"
warmup="${3:-1}"
out="${4:-/tmp/ctox_qwen35_tiled_qk_mps_grid_${tokens}_$(date -u +%Y%m%dT%H%M%SZ).txt}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

{
  echo "tiled_attention_qk_mps_grid"
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "tokens: $tokens"
  echo "iterations: $iterations"
  echo "warmup: $warmup"
  echo
  for q_tile in 64 128 256; do
    for k_tile in 512 1024; do
      echo "## candidate q_tile=$q_tile k_tile=$k_tile"
      tools/run_tiled_attention_qk_mps_prototype.sh \
        "$tokens" "$q_tile" "$k_tile" "$iterations" "$warmup"
      echo
    done
  done
} | tee "$out"

echo "$out"
