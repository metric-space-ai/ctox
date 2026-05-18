#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/run_decode_regression_matrix.sh [--dry-run] [--sizes CSV] [--iterations N] [--rounds N] <metalpack-dir>

Runs a serial end-to-end decode regression matrix for Qwen3.5-0.8B.

This is a promotion guard, not a microbenchmark. It compares realistic decode
lengths against the accepted profile and the local llama.cpp references.

Default sizes:
  128,512

Default variants:
  accepted
  no_splitk
  rowcache
  no_splitk_rowcache

Optional --storage-sweep variants:
  shared_accepted
  shared_no_splitk

Optional --sync-sweep variants:
  async_accepted
  async_no_splitk

Rounds alternate variant order to reduce thermal/scheduler bias. For promotion,
use at least --iterations 3 --rounds 2 and keep benchmarks serial.

Examples:
  tools/run_decode_regression_matrix.sh /tmp/ctox_qwen35_08b_real_fp16.metalpack
  tools/run_decode_regression_matrix.sh --dry-run --sizes 128 /tmp/ctox_qwen35_08b_real_fp16.metalpack
USAGE
}

dry_run=0
sizes_csv="128,512"
iterations=1
rounds=1
storage_sweep=0
sync_sweep=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      dry_run=1
      shift
      ;;
    --sizes)
      if [[ $# -lt 2 ]]; then
        usage
        exit 2
      fi
      sizes_csv="$2"
      shift 2
      ;;
    --iterations)
      if [[ $# -lt 2 ]]; then
        usage
        exit 2
      fi
      iterations="$2"
      shift 2
      ;;
    --rounds)
      if [[ $# -lt 2 ]]; then
        usage
        exit 2
      fi
      rounds="$2"
      shift 2
      ;;
    --storage-sweep)
      storage_sweep=1
      shift
      ;;
    --sync-sweep)
      sync_sweep=1
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

metalpack="$1"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runner="$repo_root/tools/run_accepted_profile.sh"
bench="$repo_root/target/release/bench_metalpack_decode_layered_pattern"

if [[ ! -d "$metalpack" ]]; then
  echo "missing metalpack directory: $metalpack" >&2
  exit 2
fi

if [[ ! -x "$bench" ]]; then
  echo "missing benchmark binary: $bench" >&2
  echo "build it with: cargo build --release --bin bench_metalpack_decode_layered_pattern" >&2
  exit 2
fi

if ! [[ "$iterations" =~ ^[0-9]+$ ]] || [[ "$iterations" -lt 1 ]]; then
  echo "iterations must be a positive integer" >&2
  exit 2
fi
if ! [[ "$rounds" =~ ^[0-9]+$ ]] || [[ "$rounds" -lt 1 ]]; then
  echo "rounds must be a positive integer" >&2
  exit 2
fi

lock_dir="/tmp/ctox_qwen35_decode_regression_matrix.lockdir"
if ! mkdir "$lock_dir" 2>/dev/null; then
  echo "decode regression matrix lock is held: $lock_dir" >&2
  echo "do not run decode benchmarks in parallel" >&2
  exit 1
fi
cleanup() {
  rm -rf "$lock_dir"
}
trap cleanup EXIT INT TERM

llama_ref_tps() {
  case "$1" in
    128) echo "52.98" ;;
    512) echo "44.77" ;;
    *) echo "n/a" ;;
  esac
}

variant_env() {
  case "$1" in
    accepted) ;;
    shared_accepted)
      echo "CTOX_QWEN35_SHARED_WEIGHTS=1"
      ;;
    no_splitk)
      echo "CTOX_QWEN35_DECODE_ATTENTION_NO_SPLITK=1"
      ;;
    async_accepted)
      echo "CTOX_QWEN35_DECODE_ASYNC_COMMANDS=1"
      ;;
    shared_no_splitk)
      echo "CTOX_QWEN35_SHARED_WEIGHTS=1"
      echo "CTOX_QWEN35_DECODE_ATTENTION_NO_SPLITK=1"
      ;;
    async_no_splitk)
      echo "CTOX_QWEN35_DECODE_ASYNC_COMMANDS=1"
      echo "CTOX_QWEN35_DECODE_ATTENTION_NO_SPLITK=1"
      ;;
    rowcache)
      echo "CTOX_QWEN35_DECODE_DELTA_ROWCACHE=1"
      ;;
    no_splitk_rowcache)
      echo "CTOX_QWEN35_DECODE_ATTENTION_NO_SPLITK=1"
      echo "CTOX_QWEN35_DECODE_DELTA_ROWCACHE=1"
      ;;
    *)
      echo "unknown variant: $1" >&2
      exit 2
      ;;
  esac
}

run_one() {
  local round="$1"
  shift
  local variant="$1"
  local size="$2"
  local -a envs=()
  while IFS= read -r env_line; do
    [[ -n "$env_line" ]] && envs+=("$env_line")
  done < <(variant_env "$variant")

  local -a cmd=()
  if [[ "${#envs[@]}" -gt 0 ]]; then
    cmd=(env "${envs[@]}")
  fi
  cmd+=(
    "$runner"
    "$bench"
    "$metalpack"
    ignored ignored ignored
    107 "$iterations" 0 "$size" "$size"
  )

  if [[ "$dry_run" -eq 1 ]]; then
    printf 'dry_run round=%s variant=%s size=%s command:' "$round" "$variant" "$size"
    printf ' %q' "${cmd[@]}"
    printf '\n'
    return
  fi

  local output median tps ref ratio status
  set +e
  output="$("${cmd[@]}" 2>&1)"
  status=$?
  set -e
  if [[ "$status" -ne 0 ]]; then
    echo "$output" >&2
    echo "variant=$variant size=$size failed with exit code $status" >&2
    exit "$status"
  fi
  median="$(awk -F': ' '/^median_s:/ {print $2; exit}' <<<"$output")"
  if [[ -z "$median" ]]; then
    echo "$output" >&2
    echo "could not parse median_s for variant=$variant size=$size" >&2
    exit 1
  fi
  tps="$(awk -v tokens="$size" -v seconds="$median" 'BEGIN { printf "%.2f", tokens / seconds }')"
  ref="$(llama_ref_tps "$size")"
  if [[ "$ref" == "n/a" ]]; then
    ratio="n/a"
  else
    ratio="$(awk -v tps="$tps" -v ref="$ref" 'BEGIN { printf "%.3f", tps / ref }')"
  fi
  printf 'round=%-3s %-20s size=%-5s median_s=%-13s tok_s=%-8s llama_ref=%-8s ratio=%s\n' \
    "$round" "$variant" "$size" "$median" "$tps" "$ref" "$ratio"
}

IFS=',' read -r -a sizes <<<"$sizes_csv"
variants=(accepted no_splitk rowcache no_splitk_rowcache)
if [[ "$storage_sweep" -eq 1 ]]; then
  variants+=(shared_accepted shared_no_splitk)
fi
if [[ "$sync_sweep" -eq 1 ]]; then
  variants+=(async_accepted async_no_splitk)
fi

echo "decode_regression_matrix"
echo "metalpack: $metalpack"
echo "iterations: $iterations"
echo "rounds: $rounds"
echo "sizes: $sizes_csv"
echo "storage_sweep: $storage_sweep"
echo "sync_sweep: $sync_sweep"
echo "llama_refs: tg128=52.98 tok/s tg512=44.77 tok/s"

for ((round = 1; round <= rounds; round++)); do
  if (( round % 2 == 1 )); then
    round_variants=("${variants[@]}")
  else
    round_variants=()
    for ((idx = ${#variants[@]} - 1; idx >= 0; idx--)); do
      round_variants+=("${variants[$idx]}")
    done
  fi
  for size in "${sizes[@]}"; do
    if ! [[ "$size" =~ ^[0-9]+$ ]] || [[ "$size" -lt 1 ]]; then
      echo "invalid size: $size" >&2
      exit 2
    fi
    for variant in "${round_variants[@]}"; do
      run_one "$round" "$variant" "$size"
    done
  done
done
