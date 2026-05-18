#!/usr/bin/env bash
set -euo pipefail

tokens="${1:-512}"
rows="${2:-3584}"
iterations="${3:-3}"
warmup="${4:-1}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "static_int8_matmul_autotune"
echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "tokens: $tokens"
echo "rows: $rows"
echo "cols: 1024"
echo "iterations: $iterations"
echo "warmup: $warmup"

for kernel in scalar simd32; do
  for row_tile in 4 8 16; do
    for quant_group_size in 64 128 256; do
      echo
      echo "## candidate kernel=$kernel row_tile=$row_tile quant_group_size=$quant_group_size col_tile=256"
      target/release/bench_static_int8_matmul \
        "$tokens" "$rows" "$iterations" "$warmup" "$quant_group_size" "$row_tile" 256 "$kernel"
    done
  done
done
