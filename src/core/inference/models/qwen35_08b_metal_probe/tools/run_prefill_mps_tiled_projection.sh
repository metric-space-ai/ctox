#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/run_prefill_mps_tiled_projection.sh [--sizes CSV] [--layer N] [--iters N] <metalpack-dir>

Measure the accepted QH4 prefill attention core and the exact MPS tiled
attention candidate, then project full-prefill impact by replacing Qwen's six
full-attention layers in the current exact_mps_deltaout baseline.

This is a projection tool, not an accepted-profile benchmark. Promotion still
requires full-prefill wiring and parity gates.
USAGE
}

sizes_csv="4096,16384,32768"
layer="3"
iters="2"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sizes)
      sizes_csv="${2:?missing --sizes value}"
      shift 2
      ;;
    --layer)
      layer="${2:?missing --layer value}"
      shift 2
      ;;
    --iters)
      iters="${2:?missing --iters value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "unknown option: $1" >&2
      usage
      exit 2
      ;;
    *)
      break
      ;;
  esac
done

if [[ $# -ne 1 ]]; then
  usage
  exit 2
fi

metalpack="$1"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bench="$repo_root/target/release/bench_metalpack_prefill_attention_core"
accepted_runner="$repo_root/tools/run_accepted_profile.sh"

if [[ ! -x "$bench" ]]; then
  echo "missing bench binary: $bench" >&2
  echo "build it with: cargo build --release --bin bench_metalpack_prefill_attention_core" >&2
  exit 1
fi

extract_metric() {
  local key="$1"
  awk -v key="$key" '$1 == key ":" { print $2; found=1 } END { if (!found) exit 1 }'
}

project_row() {
  python3 - "$@" <<'PY'
import sys
tokens = int(sys.argv[1])
accepted = float(sys.argv[2])
mps = float(sys.argv[3])
baseline = float(sys.argv[4])
llama = float(sys.argv[5])
projected = baseline - 6.0 * (accepted - mps)
tok_s = tokens / projected
vs = tok_s / llama
speedup = accepted / mps
print(
    f"tokens={tokens:<7} accepted_attention_s={accepted:.9f} "
    f"mps_tiled_attention_s={mps:.9f} attention_speedup={speedup:.2f}x "
    f"projected_full_s={projected:.9f} projected_tok_s={tok_s:.2f} "
    f"llama_tok_s={llama:.2f} projected_vs_llama={vs:.3f}x"
)
PY
}

baseline_seconds() {
  case "$1" in
    4096) echo "1.316" ;;
    16384) echo "11.733" ;;
    32768) echo "41.642" ;;
    *)
      echo "no exact_mps_deltaout baseline for tokens=$1" >&2
      return 1
      ;;
  esac
}

llama_tok_s() {
  case "$1" in
    4096) echo "2852.70" ;;
    16384) echo "2065.71" ;;
    32768) echo "1325.20" ;;
    *)
      echo "no llama.cpp reference for tokens=$1" >&2
      return 1
      ;;
  esac
}

IFS=',' read -r -a sizes <<<"$sizes_csv"

echo "prefill_mps_tiled_projection"
echo "metalpack: $metalpack"
echo "layer: $layer"
echo "iters: $iters"
echo "contract: live attention-core measurements + static exact_mps_deltaout full-prefill baseline"

for tokens in "${sizes[@]}"; do
  baseline="$(baseline_seconds "$tokens")"
  llama="$(llama_tok_s "$tokens")"
  accepted_out="$("$accepted_runner" "$bench" "$metalpack" "$layer" "$tokens" "$iters" 1)"
  accepted_s="$(printf '%s\n' "$accepted_out" | extract_metric median_s)"
  mps_out="$(
    source "$repo_root/docs/kernel-dev/accepted_profile.env"
    unset CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8
    export CTOX_QWEN35_ATTENTION_MPS_TILED=1
    "$bench" "$metalpack" "$layer" "$tokens" "$iters" 1
  )"
  mps_s="$(printf '%s\n' "$mps_out" | extract_metric median_s)"
  project_row "$tokens" "$accepted_s" "$mps_s" "$baseline" "$llama"
done
