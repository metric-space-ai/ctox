#!/usr/bin/env bash
set -euo pipefail

tokens="${1:-2048}"
chunk="${2:-32}"
iterations="${3:-5}"
warmup="${4:-2}"
out_dir="${5:-/tmp/ctox_qwen35_quant_delta_scan_$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$out_dir"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo build --release --bin bench_deltanet_chunk_phase2 >/dev/null

target/release/bench_deltanet_chunk_phase2 "$tokens" "$iterations" "$warmup" "$chunk" f32x4 \
  > "$out_dir/f32x4.txt"
target/release/bench_deltanet_chunk_phase2 "$tokens" "$iterations" "$warmup" "$chunk" f16x4 \
  > "$out_dir/f16x4.txt"

python3 - "$out_dir/f32x4.txt" "$out_dir/f16x4.txt" "$out_dir/combined_metrics.txt" <<'PY'
import re
import sys
from pathlib import Path

def read(path):
    metrics = {}
    for line in Path(path).read_text().splitlines():
        match = re.match(r"^([A-Za-z0-9_.-]+):\s*([-+0-9.eE]+)\s*$", line.strip())
        if match:
            metrics[match.group(1)] = float(match.group(2))
    return metrics

baseline = read(sys.argv[1])
candidate = read(sys.argv[2])
out = Path(sys.argv[3])
out.write_text(
    "\n".join(
        [
            f"baseline_median_s: {baseline['full_path_median_s']:.9f}",
            f"candidate_median_s: {candidate['full_path_median_s']:.9f}",
            f"max_abs_error: {candidate['max_abs_error_full_out']:.9f}",
            f"mean_abs_error: {candidate['mean_abs_error_full_out']:.9f}",
            f"max_abs_error_full_state: {candidate['max_abs_error_full_state']:.9f}",
            f"mean_abs_error_full_state: {candidate['mean_abs_error_full_state']:.9f}",
        ]
    )
    + "\n"
)
PY

tools/quant_error_gate.py "$out_dir/combined_metrics.txt" \
  --max-abs 0.00002 \
  --mean-abs 0.000001 \
  --speedup-min 1.01 \
  > "$out_dir/quant_gate.txt" || true

{
  echo "# Quantized Delta Scan Gate"
  echo
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "tokens: $tokens"
  echo "chunk: $chunk"
  echo "iterations: $iterations"
  echo "warmup: $warmup"
  echo
  echo "## f32x4 baseline"
  cat "$out_dir/f32x4.txt"
  echo
  echo "## f16x4 candidate"
  cat "$out_dir/f16x4.txt"
  echo
  echo "## quant gate"
  cat "$out_dir/quant_gate.txt"
} > "$out_dir/report.md"

cat "$out_dir/report.md"
echo "$out_dir"
