#!/usr/bin/env bash
set -euo pipefail

tokens_csv="${1:-4096,8192,16384}"
iterations="${2:-3}"
warmup="${3:-1}"
out_dir="${4:-/tmp/ctox_qwen35_attention_qk_mps_$(date -u +%Y%m%dT%H%M%SZ)}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
mkdir -p "$out_dir"

{
  echo "attention_qk_mps_probe"
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "tokens_csv: $tokens_csv"
  echo "head_dim: 256"
  echo "iterations: $iterations"
  echo "warmup: $warmup"
  echo "contract: dense QK probe only; not a full exact attention implementation"
  echo
  IFS=',' read -r -a token_values <<< "$tokens_csv"
  for tokens in "${token_values[@]}"; do
    tokens="${tokens//[[:space:]]/}"
    [[ -n "$tokens" ]] || continue
    echo "## qk tokens=$tokens"
    tools/run_mps_matrix_probe.sh "$tokens" "$tokens" 256 "$iterations" "$warmup" \
      | tee "$out_dir/qk_${tokens}.txt"
    echo
  done
} | tee "$out_dir/report.md"

echo "$out_dir"
