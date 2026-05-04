#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/run_delta_scan_family_sweep.sh [options]

Runs serial, reset-based Delta scan family comparisons on the current MPS
sidecar pipeline. This is intentionally serial: do not parallelize benchmark
runs that share the GPU/thermal envelope.

Options:
  --metalpack PATH        default: /tmp/ctox_qwen35_08b_real_fp16.metalpack
  --tokens N[,N...]      default: 512,4096,16384
  --rounds N             default: 1
  --iterations N         default: 1
  --warmup N             default: 1
  --mps-ffn-sidecar DIR  default: /tmp/ctox_qwen35_mps_ffn_sidecar
  --mps-delta-project-sidecar DIR
                          default: /tmp/ctox_qwen35_mps_delta_project_sidecar
  --mps-delta-out-sidecar DIR
                          default: /tmp/ctox_qwen35_mps_delta_out_sidecar
  --output-dir DIR       default: /tmp/ctox_qwen35_scan_sweep_<pid>
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
metalpack="/tmp/ctox_qwen35_08b_real_fp16.metalpack"
tokens="512,4096,16384"
rounds=1
iterations=1
warmup=1
mps_ffn_sidecar="/tmp/ctox_qwen35_mps_ffn_sidecar"
mps_delta_project_sidecar="/tmp/ctox_qwen35_mps_delta_project_sidecar"
mps_delta_out_sidecar="/tmp/ctox_qwen35_mps_delta_out_sidecar"
output_dir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --metalpack)
      metalpack="${2:-}"
      shift 2
      ;;
    --tokens)
      tokens="${2:-}"
      shift 2
      ;;
    --rounds)
      rounds="${2:-}"
      shift 2
      ;;
    --iterations)
      iterations="${2:-}"
      shift 2
      ;;
    --warmup)
      warmup="${2:-}"
      shift 2
      ;;
    --mps-ffn-sidecar)
      mps_ffn_sidecar="${2:-}"
      shift 2
      ;;
    --mps-delta-project-sidecar)
      mps_delta_project_sidecar="${2:-}"
      shift 2
      ;;
    --mps-delta-out-sidecar)
      mps_delta_out_sidecar="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

for path in "$metalpack" "$mps_ffn_sidecar" "$mps_delta_project_sidecar" "$mps_delta_out_sidecar"; do
  if [[ ! -d "$path" ]]; then
    echo "missing directory: $path" >&2
    exit 2
  fi
done

compare="$repo_root/tools/compare_delta_stack_candidate.sh"
if [[ ! -x "$compare" ]]; then
  echo "missing executable: $compare" >&2
  exit 2
fi

output_dir="${output_dir:-/tmp/ctox_qwen35_scan_sweep_$$}"
mkdir -p "$output_dir"
summary="$output_dir/summary.txt"
: > "$summary"

base_env=(
  --candidate-env CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128=1
  --candidate-env CTOX_QWEN35_DELTA_OUT_MMA64=1
  --candidate-env CTOX_QWEN35_FFN_GATE_UP_MMA64=1
  --candidate-env CTOX_QWEN35_DOWN_MMA64=1
  --candidate-env CTOX_QWEN35_DOWN_MMA64_RESIDUAL=1
  --candidate-env CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED=1
)

run_candidate() {
  local name="$1"
  shift
  local candidate_dir="$output_dir/$name"
  mkdir -p "$candidate_dir"
  echo "== $name ==" | tee -a "$summary"
  "$compare" --candidate-reset-tuning-env \
    "${base_env[@]}" \
    "$@" \
    --tokens "$tokens" \
    --rounds "$rounds" \
    --iterations "$iterations" \
    --warmup "$warmup" \
    --mps-ffn-sidecar "$mps_ffn_sidecar" \
    --mps-delta-project-sidecar "$mps_delta_project_sidecar" \
    --mps-delta-out-sidecar "$mps_delta_out_sidecar" \
    --output-dir "$candidate_dir" | tee -a "$summary"
}

run_candidate rowcache \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1

run_candidate rowcache_direct \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE_DIRECT=1

run_candidate rowcache_block64 \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64=1

run_candidate rowcache_block32 \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1

run_candidate rowcache_block_auto \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1

run_candidate lanes4_sharedqk_approx \
  --candidate-env CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1

echo "summary: $summary"
