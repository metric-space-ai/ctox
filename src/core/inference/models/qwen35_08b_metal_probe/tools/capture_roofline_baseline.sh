#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/capture_roofline_baseline.sh [options]

Captures device-local operational roofline baselines for this crate. The output
is a directory with one captured run per probe plus summary.csv and roofline.env.

Options:
  --output-dir DIR       output directory (default: /tmp/ctox_qwen35_roofline_<utc>)
  --stream-mib LIST     comma-separated stream sizes in MiB (default: 64,256,512)
  --stream-iters N      iterations for bench_stream (default: 7)
  --prefill-tokens N    tokens for prefill matmul proxies (default: 4096)
  --prefill-iters N     iterations for prefill matmul proxies (default: 3)
  --no-build            do not run cargo build --release --bins first

This script runs benchmarks. Do not run it concurrently with other benchmark
workloads.
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
output_dir="/tmp/ctox_qwen35_roofline_${timestamp}"
stream_mib_list="64,256,512"
stream_iters=7
prefill_tokens=4096
prefill_iters=3
do_build=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-dir)
      output_dir="${2:-}"
      shift 2
      ;;
    --stream-mib)
      stream_mib_list="${2:-}"
      shift 2
      ;;
    --stream-iters)
      stream_iters="${2:-}"
      shift 2
      ;;
    --prefill-tokens)
      prefill_tokens="${2:-}"
      shift 2
      ;;
    --prefill-iters)
      prefill_iters="${2:-}"
      shift 2
      ;;
    --no-build)
      do_build=0
      shift
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

if [[ -z "$output_dir" ]]; then
  echo "--output-dir cannot be empty" >&2
  exit 2
fi
if [[ ! "$stream_mib_list" =~ ^[0-9]+(,[0-9]+)*$ ]]; then
  echo "--stream-mib must be a comma-separated integer list" >&2
  exit 2
fi
for value in "$stream_iters" "$prefill_tokens" "$prefill_iters"; do
  if [[ ! "$value" =~ ^[0-9]+$ || "$value" -eq 0 ]]; then
    echo "numeric options must be positive integers" >&2
    exit 2
  fi
done

mkdir -p "$output_dir"

if [[ "$do_build" -eq 1 ]]; then
  cargo build --release --bins
fi

capture() {
  local label="$1"
  shift
  tools/capture_measurement_output.sh \
    --accepted-profile \
    --output-dir "$output_dir" \
    --label "$label" \
    -- "$@"
}

IFS=, read -r -a stream_sizes <<<"$stream_mib_list"
for mib in "${stream_sizes[@]}"; do
  capture "roofline-stream-${mib}mib" target/release/bench_stream "$mib" "$stream_iters"
done

capture "roofline-prefill-rmsmatmul-3584" \
  target/release/bench_prefill_rms_matmul "$prefill_tokens" 3584 "$prefill_iters"
capture "roofline-prefill-rmsmatmul-1024" \
  target/release/bench_prefill_rms_matmul "$prefill_tokens" 1024 "$prefill_iters"
capture "roofline-matvec-tiled-3584" \
  target/release/bench_matvec_tiled 3584 "$prefill_iters"

summary="$output_dir/summary.csv"
{
  echo "label,command,tokens_context,iterations,median_s,p95_s,effective_GB_s,stdout"
  while IFS= read -r manifest; do
    run_dir="$(dirname "$manifest")"
    normalized="$run_dir/normalized.txt"
    field() {
      local file="$1"
      local key="$2"
      awk -v key="$key" '
        index($0, key ":") == 1 {
          sub(/^[^:]*:[ \t]*/, "")
          sub(/[ \t]+$/, "")
          print
          exit
        }
      ' "$file"
    }
    label="$(field "$manifest" label)"
    command="$(field "$manifest" command)"
    tokens="$(field "$normalized" tokens/context)"
    iterations="$(field "$normalized" iterations)"
    median="$(field "$normalized" median_s)"
    p95="$(field "$normalized" p95_s)"
    eff="$(field "$normalized" effective_GB/s)"
    stdout="$(field "$manifest" stdout)"
    printf '"%s","%s","%s","%s","%s","%s","%s","%s"\n' \
      "$label" "$command" "$tokens" "$iterations" "$median" "$p95" "$eff" "$stdout"
  done < <(find "$output_dir" -mindepth 2 -maxdepth 2 -name manifest.txt | sort)
} > "$summary"

roofline_env="$output_dir/roofline.env"
awk -F, '
  NR > 1 {
    label = $1
    eff = $7
    gsub(/^"|"$/, "", label)
    gsub(/^"|"$/, "", eff)
    if (label ~ /^roofline-stream-/ && eff + 0 > max_stream) max_stream = eff + 0
    if (label ~ /^roofline-prefill-/ && eff + 0 > max_prefill) max_prefill = eff + 0
    if (label ~ /^roofline-matvec-/ && eff + 0 > max_matvec) max_matvec = eff + 0
  }
  END {
    printf "sustained_stream_GB_s=%.6f\n", max_stream
    printf "operational_prefill_matmul_GB_s=%.6f\n", max_prefill
    printf "operational_matvec_GB_s=%.6f\n", max_matvec
  }
' "$summary" > "$roofline_env"

echo "roofline_dir: $output_dir"
echo "summary:      $summary"
echo "roofline_env: $roofline_env"
cat "$roofline_env"
