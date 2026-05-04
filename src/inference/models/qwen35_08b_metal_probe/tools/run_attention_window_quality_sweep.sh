#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  cat >&2 <<'USAGE'
usage: tools/run_attention_window_quality_sweep.sh <metalpack-dir> <tokens> [layer=3] [iterations=1] [mps-attention-out-sidecar-dir] [windows_csv=2048,4096,8192,16384]

Runs serial benchmarks only. It dumps the exact qh4 SIMD32 vec8 attention output once,
then runs each windowed candidate and compares the raw attention tensor against exact.
USAGE
  exit 2
fi

metalpack="$1"
tokens="$2"
layer="${3:-3}"
iterations="${4:-1}"
mps_attention_out="${5:-}"
windows_csv="${6:-2048,4096,8192,16384}"

bin_dir="target/release"
bench="$bin_dir/bench_metalpack_prefill_attention_core"
compare="$bin_dir/compare_attention_raw_dump"
if [[ ! -x "$bench" || ! -x "$compare" ]]; then
  echo "missing release binaries; run cargo build --release --bins first" >&2
  exit 2
fi

tmpdir="$(mktemp -d /tmp/ctox_qwen35_attention_window_sweep.XXXXXX)"
trap 'rm -rf "$tmpdir"' EXIT

width=2048
baseline_dump="$tmpdir/exact.bin"

echo "attention_window_quality_sweep"
echo "metalpack: $metalpack"
echo "layer: $layer"
echo "tokens: $tokens"
echo "iterations: $iterations"
if [[ -n "$mps_attention_out" ]]; then
  echo "mps_attention_out_sidecar: $mps_attention_out"
fi
echo "windows: $windows_csv"
echo

echo "record_type,variant,window,median_s,p95_s,effective_gb_s,checksum,mean_abs_error,rms_error,max_abs_error,checksum_delta,mismatch_count"

exact_out="$tmpdir/exact.out"
if [[ -n "$mps_attention_out" ]]; then
  CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1 \
  CTOX_QWEN35_ATTENTION_RAW_DUMP="$baseline_dump" \
    "$bench" "$metalpack" "$layer" "$tokens" "$iterations" 1 "$mps_attention_out" > "$exact_out"
else
  CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8=1 \
  CTOX_QWEN35_ATTENTION_RAW_DUMP="$baseline_dump" \
    "$bench" "$metalpack" "$layer" "$tokens" "$iterations" 1 > "$exact_out"
fi

python3 - "$exact_out" <<'PY'
import sys
path = sys.argv[1]
metrics = {}
for line in open(path, encoding="utf-8"):
    if ":" in line:
        k, v = line.split(":", 1)
        try:
            metrics[k.strip()] = float(v.strip())
        except ValueError:
            pass
print("bench,exact,full,{median_s:.9f},{p95_s:.9f},{gb:.2f},{checksum:.6f},0,0,0,0,0".format(
    median_s=metrics["median_s"],
    p95_s=metrics["p95_s"],
    gb=metrics["effective_gb_s_attention_core_estimate"],
    checksum=metrics["checksum16"],
))
PY

IFS=',' read -r -a windows <<< "$windows_csv"
for window in "${windows[@]}"; do
  window="${window//[[:space:]]/}"
  [[ -n "$window" ]] || continue
  candidate_dump="$tmpdir/window_${window}.bin"
  candidate_out="$tmpdir/window_${window}.out"
  compare_out="$tmpdir/window_${window}.compare"
  if [[ -n "$mps_attention_out" ]]; then
    CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW="$window" \
    CTOX_QWEN35_ATTENTION_RAW_DUMP="$candidate_dump" \
      "$bench" "$metalpack" "$layer" "$tokens" "$iterations" 1 "$mps_attention_out" > "$candidate_out"
  else
    CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8_WINDOW="$window" \
    CTOX_QWEN35_ATTENTION_RAW_DUMP="$candidate_dump" \
      "$bench" "$metalpack" "$layer" "$tokens" "$iterations" 1 > "$candidate_out"
  fi
  "$compare" "$baseline_dump" "$candidate_dump" "$tokens" "$width" > "$compare_out"
  python3 - "$candidate_out" "$compare_out" "$window" <<'PY'
import sys
bench_path, compare_path, window = sys.argv[1:]
bench = {}
cmp = {}
for line in open(bench_path, encoding="utf-8"):
    if ":" in line:
        k, v = line.split(":", 1)
        try:
            bench[k.strip()] = float(v.strip())
        except ValueError:
            pass
for line in open(compare_path, encoding="utf-8"):
    if ":" in line:
        k, v = line.split(":", 1)
        try:
            cmp[k.strip()] = float(v.strip())
        except ValueError:
            pass
print("bench,window,{window},{median_s:.9f},{p95_s:.9f},{gb:.2f},{checksum:.6f},{mean:.9f},{rms:.9f},{max_abs:.9f},{checksum_delta:.9f},{mismatch:.0f}".format(
    window=window,
    median_s=bench["median_s"],
    p95_s=bench["p95_s"],
    gb=bench["effective_gb_s_attention_core_estimate"],
    checksum=bench["checksum16"],
    mean=cmp["mean_abs_error"],
    rms=cmp["rms_error"],
    max_abs=cmp["max_abs_error"],
    checksum_delta=cmp["checksum_delta"],
    mismatch=cmp["mismatch_count"],
))
PY
done
