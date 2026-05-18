#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/run_mps_tiled_attention_parity_sweep.sh [--tokens N] [--iters N] [--layers CSV] <metalpack-dir>

Compare accepted QH4 attention raw dumps with the exact MPS tiled attention
candidate across Qwen full-attention layers. The comparison is FP16-tolerant;
bitwise equality is not expected because MPSMatrix changes accumulation order.
USAGE
}

tokens="512"
iters="1"
layers_csv="3,7,11,15,19,23"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tokens)
      tokens="${2:?missing --tokens value}"
      shift 2
      ;;
    --iters)
      iters="${2:?missing --iters value}"
      shift 2
      ;;
    --layers)
      layers_csv="${2:?missing --layers value}"
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
compare="$repo_root/target/release/compare_attention_raw_dump"
accepted_runner="$repo_root/tools/run_accepted_profile.sh"

for exe in "$bench" "$compare"; do
  if [[ ! -x "$exe" ]]; then
    echo "missing binary: $exe" >&2
    echo "build with: cargo build --release --bin bench_metalpack_prefill_attention_core --bin compare_attention_raw_dump" >&2
    exit 1
  fi
done

extract_metric() {
  local key="$1"
  awk -v key="$key" '$1 == key ":" { print $2; found=1 } END { if (!found) exit 1 }'
}

tmpdir="$(mktemp -d /tmp/ctox_mps_tiled_attention_parity.XXXXXX)"
trap 'rm -rf "$tmpdir"' EXIT

IFS=',' read -r -a layers <<<"$layers_csv"

echo "mps_tiled_attention_parity_sweep"
echo "metalpack: $metalpack"
echo "tokens: $tokens"
echo "iters: $iters"
echo "layers: $layers_csv"

for layer in "${layers[@]}"; do
  base_dump="$tmpdir/layer_${layer}_accepted.bin"
  mps_dump="$tmpdir/layer_${layer}_mps.bin"
  accepted_out="$(
    CTOX_QWEN35_ATTENTION_RAW_DUMP="$base_dump" \
      "$accepted_runner" "$bench" "$metalpack" "$layer" "$tokens" "$iters" 1
  )"
  mps_out="$(
    source "$repo_root/docs/kernel-dev/accepted_profile.env"
    unset CTOX_QWEN35_ATTENTION_QH4_SIMD32_VEC8
    export CTOX_QWEN35_ATTENTION_MPS_TILED=1
    export CTOX_QWEN35_ATTENTION_RAW_DUMP="$mps_dump"
    "$bench" "$metalpack" "$layer" "$tokens" "$iters" 1
  )"
  accepted_s="$(printf '%s\n' "$accepted_out" | extract_metric median_s)"
  mps_s="$(printf '%s\n' "$mps_out" | extract_metric median_s)"
  compare_out="$("$compare" "$base_dump" "$mps_dump" "$tokens" 2048)"
  mean_abs="$(printf '%s\n' "$compare_out" | extract_metric mean_abs_error)"
  rms="$(printf '%s\n' "$compare_out" | extract_metric rms_error)"
  max_abs="$(printf '%s\n' "$compare_out" | extract_metric max_abs_error)"
  checksum_delta="$(printf '%s\n' "$compare_out" | extract_metric checksum_delta)"
  python3 - "$layer" "$accepted_s" "$mps_s" "$mean_abs" "$rms" "$max_abs" "$checksum_delta" <<'PY'
import sys
layer = sys.argv[1]
accepted = float(sys.argv[2])
mps = float(sys.argv[3])
mean_abs = float(sys.argv[4])
rms = float(sys.argv[5])
max_abs = float(sys.argv[6])
checksum_delta = float(sys.argv[7])
speedup = accepted / mps if mps else 0.0
print(
    f"layer={layer:<2} accepted_s={accepted:.9f} mps_s={mps:.9f} "
    f"speedup={speedup:.2f}x mean_abs={mean_abs:.9f} rms={rms:.9f} "
    f"max_abs={max_abs:.9f} checksum_delta={checksum_delta:.9f}"
)
PY
done
