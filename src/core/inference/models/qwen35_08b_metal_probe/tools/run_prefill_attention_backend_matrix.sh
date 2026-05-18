#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/run_prefill_attention_backend_matrix.sh [--sizes CSV] [--layer N] [--accepted-iters N] [--tiled-iters N] [--dry-run] <metalpack-dir>

Runs a serial prefill-attention backend matrix:
  1. accepted Metal attention-core benchmark on real metalpack weights
  2. Rust MPS tiled QK-softmax-PV prototype with synthetic Qwen-layout bridge

Important: these numbers are not semantically interchangeable. The accepted
benchmark includes the current real prefill attention core path. The MPS tiled
prototype uses synthetic accepted-layout q_cache/k_cache/v_cache, packs both
Qwen KV groups into MPS-friendly scratch matrices, and runs the tiled inner
exact attention core. It does not include real QKV projection, O projection,
full model wiring, or full hidden-dump parity yet.

Use this tool to decide whether the MPS tiled path is worth integrating, not to
claim accepted-profile performance.
USAGE
}

sizes_csv="4096,16384,32768"
layer=3
accepted_iters=3
tiled_iters=3
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sizes)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      sizes_csv="$2"
      shift 2
      ;;
    --layer)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      layer="$2"
      shift 2
      ;;
    --accepted-iters)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      accepted_iters="$2"
      shift 2
      ;;
    --tiled-iters)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      tiled_iters="$2"
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    -h|--help)
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

if ! [[ "$layer" =~ ^[0-9]+$ ]]; then
  echo "layer must be a non-negative integer" >&2
  exit 2
fi
if ! [[ "$accepted_iters" =~ ^[0-9]+$ ]] || [[ "$accepted_iters" -lt 1 ]]; then
  echo "accepted-iters must be a positive integer" >&2
  exit 2
fi
if ! [[ "$tiled_iters" =~ ^[0-9]+$ ]] || [[ "$tiled_iters" -lt 1 ]]; then
  echo "tiled-iters must be a positive integer" >&2
  exit 2
fi

metalpack="$1"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
accepted_bench="$repo_root/target/release/bench_metalpack_prefill_attention_core"
tiled_bench="$repo_root/target/release/bench_tiled_attention_mps"
runner="$repo_root/tools/run_accepted_profile.sh"

if [[ ! -d "$metalpack" ]]; then
  echo "missing metalpack directory: $metalpack" >&2
  exit 2
fi
if [[ ! -x "$accepted_bench" ]]; then
  echo "missing benchmark binary: $accepted_bench" >&2
  echo "build it with: cargo build --release --bin bench_metalpack_prefill_attention_core" >&2
  exit 2
fi
if [[ ! -x "$tiled_bench" ]]; then
  echo "missing benchmark binary: $tiled_bench" >&2
  echo "build it with: cargo build --release --bin bench_tiled_attention_mps" >&2
  exit 2
fi

lock_dir="/tmp/ctox_qwen35_prefill_attention_backend_matrix.lockdir"
if ! mkdir "$lock_dir" 2>/dev/null; then
  echo "prefill attention backend matrix lock is held: $lock_dir" >&2
  echo "do not run performance measurements in parallel" >&2
  exit 1
fi
cleanup() {
  rm -rf "$lock_dir"
}
trap cleanup EXIT INT TERM

parse_median() {
  awk -F': ' '/^median_s:/ {print $2; exit}'
}

echo "prefill_attention_backend_matrix"
echo "metalpack: $metalpack"
echo "layer: $layer"
echo "sizes: $sizes_csv"
echo "accepted_iters: $accepted_iters"
echo "tiled_iters: $tiled_iters"
echo "contract: accepted=real current path; tiled_mps_bridge=synthetic Qwen-layout bridge for both KV groups + inner exact attention"

IFS=',' read -r -a sizes <<<"$sizes_csv"
for tokens in "${sizes[@]}"; do
  if ! [[ "$tokens" =~ ^[0-9]+$ ]] || [[ "$tokens" -lt 1 ]]; then
    echo "invalid token size: $tokens" >&2
    exit 2
  fi

  accepted_cmd=(
    "$runner"
    "$accepted_bench"
    "$metalpack"
    "$layer"
    "$tokens"
    "$accepted_iters"
    0
  )
  tiled_cmd=(
    "$tiled_bench"
    "$tokens"
    256
    1024
    "$tiled_iters"
    1
    4
    0
    1
  )

  if [[ "$dry_run" -eq 1 ]]; then
    printf 'dry_run accepted tokens=%s command:' "$tokens"
    printf ' %q' "${accepted_cmd[@]}"
    printf '\n'
    printf 'dry_run tiled_mps tokens=%s command:' "$tokens"
    printf ' %q' "${tiled_cmd[@]}"
    printf '\n'
    continue
  fi

  accepted_out="$("${accepted_cmd[@]}")"
  accepted_median="$(parse_median <<<"$accepted_out")"
  if [[ -z "$accepted_median" ]]; then
    echo "$accepted_out" >&2
    echo "failed to parse accepted median_s for tokens=$tokens" >&2
    exit 1
  fi

  tiled_out="$("${tiled_cmd[@]}")"
  tiled_median="$(parse_median <<<"$tiled_out")"
  if [[ -z "$tiled_median" ]]; then
    echo "$tiled_out" >&2
    echo "failed to parse tiled MPS median_s for tokens=$tokens" >&2
    exit 1
  fi

  ratio="$(awk -v a="$accepted_median" -v t="$tiled_median" 'BEGIN { printf "%.2f", a / t }')"
  accepted_tok_s="$(awk -v n="$tokens" -v s="$accepted_median" 'BEGIN { printf "%.2f", n / s }')"
  tiled_tok_s="$(awk -v n="$tokens" -v s="$tiled_median" 'BEGIN { printf "%.2f", n / s }')"

  printf 'tokens=%-7s accepted_s=%-13s accepted_tok_s=%-10s tiled_mps_bridge_s=%-13s tiled_mps_bridge_tok_s=%-10s accepted_over_tiled_bridge=%.2fx\n' \
    "$tokens" "$accepted_median" "$accepted_tok_s" "$tiled_median" "$tiled_tok_s" "$ratio"
done
