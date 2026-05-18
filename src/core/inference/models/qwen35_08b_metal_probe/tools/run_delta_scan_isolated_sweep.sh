#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/run_delta_scan_isolated_sweep.sh [options]

Runs serial isolated DeltaNet scan kernel sweeps. This benchmark excludes
projection, conv, gated norm, Delta out, FFN, and attention so scan layout/cache
effects are visible instead of being hidden by full-prefill noise.

Options:
  --metalpack PATH        default: /tmp/ctox_qwen35_08b_real_fp16.metalpack
  --tokens N[,N...]      default: 512,4096,16384
  --layer N              default: 0
  --rounds N             default: 3
  --iterations N         default: 5
  --warmup N             default: 3
  --validate-tokens N    default: 8
  --output-dir DIR       default: /tmp/ctox_qwen35_scan_isolated_<pid>
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
metalpack="/tmp/ctox_qwen35_08b_real_fp16.metalpack"
tokens="512,4096,16384"
layer=0
rounds=3
iterations=5
warmup=3
validate_tokens=8
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
    --layer)
      layer="${2:-}"
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
    --validate-tokens)
      validate_tokens="${2:-}"
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

if [[ ! -d "$metalpack" ]]; then
  echo "missing metalpack directory: $metalpack" >&2
  exit 2
fi

bench="$repo_root/target/release/bench_metalpack_prefill_delta_scan"
if [[ ! -x "$bench" ]]; then
  echo "missing benchmark executable: $bench" >&2
  echo "run: cargo build --release --bin bench_metalpack_prefill_delta_scan" >&2
  exit 2
fi

output_dir="${output_dir:-/tmp/ctox_qwen35_scan_isolated_$$}"
mkdir -p "$output_dir"
raw_tsv="$output_dir/raw.tsv"
summary_tsv="$output_dir/summary.tsv"
summary_txt="$output_dir/summary.txt"

scan_keys=(
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_DIRECT
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO
  CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64_MIN_TOKENS
  CTOX_QWEN35_DELTA_SCAN_LANES4
  CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK
  CTOX_QWEN35_DELTA_SCAN_LANES4_ORDERED
  CTOX_QWEN35_DELTA_SCAN_GATED_NORM
  CTOX_QWEN35_DELTA_SCAN_CHUNK_F32X4
  CTOX_QWEN35_DELTA_SCAN_CHUNK_HSTATE
  CTOX_QWEN35_DELTA_SCAN_CHUNK_TOKENS
)

echo -e "case\ttokens\tround\tmedian_s\tp95_s\tgbps\tout_err\tstate_err\tchecksum\tkernel\tgrid\tthreads\tbytes" > "$raw_tsv"

run_case() {
  local case_name="$1"
  local token_count="$2"
  local round="$3"
  shift 3
  local log="$output_dir/${case_name}_p${token_count}_r${round}.log"

  (
    for key in "${scan_keys[@]}"; do
      unset "$key"
    done
    while [[ $# -gt 0 ]]; do
      export "$1"
      shift
    done
    "$bench" "$metalpack" "$layer" "$token_count" "$iterations" "$warmup" "$validate_tokens"
  ) > "$log"

  local median p95 gbps out_err state_err checksum kernel grid threads bytes
  median="$(awk -F': ' '/^median_s:/ {print $2}' "$log")"
  p95="$(awk -F': ' '/^p95_s:/ {print $2}' "$log")"
  gbps="$(awk -F': ' '/^effective_gb_s_state_scan_estimate:/ {print $2}' "$log")"
  out_err="$(awk -F': ' '/^max_abs_error_out_validate8:/ {print $2}' "$log")"
  state_err="$(awk -F': ' '/^max_abs_error_state_validate8:/ {print $2}' "$log")"
  checksum="$(awk -F': ' '/^checksum32:/ {print $2}' "$log")"
  kernel="$(awk -F': ' '/^kernel:/ {print $2}' "$log")"
  grid="$(awk -F': ' '/^grid:/ {print $2}' "$log")"
  threads="$(awk -F': ' '/^threads:/ {print $2}' "$log")"
  bytes="$(awk -F': ' '/^bytes_moved_estimate:/ {print $2}' "$log")"
  echo -e "${case_name}\t${token_count}\t${round}\t${median}\t${p95}\t${gbps}\t${out_err}\t${state_err}\t${checksum}\t${kernel}\t${grid}\t${threads}\t${bytes}" >> "$raw_tsv"
}

IFS=',' read -r -a token_list <<< "$tokens"
cases=(
  "plain|"
  "rowcache|CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1"
  "rowcache_direct|CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 CTOX_QWEN35_DELTA_SCAN_ROWCACHE_DIRECT=1"
  "rowcache_block64|CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK64=1"
  "rowcache_block32|CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK32=1"
  "rowcache_block_auto|CTOX_QWEN35_DELTA_SCAN_ROWCACHE=1 CTOX_QWEN35_DELTA_SCAN_ROWCACHE_BLOCK_AUTO=1"
  "lanes4_sharedqk_approx|CTOX_QWEN35_DELTA_SCAN_LANES4_SHAREDQK=1"
)

for token_count in "${token_list[@]}"; do
  for round in $(seq 1 "$rounds"); do
    for case_spec in "${cases[@]}"; do
      case_name="${case_spec%%|*}"
      env_string="${case_spec#*|}"
      env_args=()
      if [[ -n "$env_string" ]]; then
        read -r -a env_args <<< "$env_string"
      fi
      echo "running ${case_name} p${token_count} round ${round}" >&2
      if [[ ${#env_args[@]} -gt 0 ]]; then
        run_case "$case_name" "$token_count" "$round" "${env_args[@]}"
      else
        run_case "$case_name" "$token_count" "$round"
      fi
    done
  done
done

awk -F'\t' '
NR == 1 { next }
{
  key = $1 "\t" $2
  count[key] += 1
  sum[key] += $4
  gbps_sum[key] += $6
  if (!(key in min) || $4 < min[key]) min[key] = $4
  if (!(key in max) || $4 > max[key]) max[key] = $4
  out_err[key] = $7
  state_err[key] = $8
  checksum[key] = $9
  kernel[key] = $10
  grid[key] = $11
  threads[key] = $12
  bytes[key] = $13
}
END {
  for (key in count) {
    split(key, parts, "\t")
    case_name = parts[1]
    token_count = parts[2]
    mean_s[key] = sum[key] / count[key]
    mean_gbps[key] = gbps_sum[key] / count[key]
    if (case_name == "rowcache_block32") {
      accepted_s[token_count] = mean_s[key]
    }
  }
  print "case\ttokens\tmean_median_s\ttok_s\tvs_block32\tbest_s\tworst_s\tmean_gbps\tout_err\tstate_err\tchecksum\tkernel\tgrid\tthreads\tbytes"
  for (key in count) {
    split(key, parts, "\t")
    token_count = parts[2]
    tok_s = token_count / mean_s[key]
    if (token_count in accepted_s) {
      speedup = accepted_s[token_count] / mean_s[key]
    } else {
      speedup = 0
    }
    print key "\t" mean_s[key] "\t" tok_s "\t" speedup "\t" min[key] "\t" max[key] "\t" mean_gbps[key] "\t" out_err[key] "\t" state_err[key] "\t" checksum[key] "\t" kernel[key] "\t" grid[key] "\t" threads[key] "\t" bytes[key]
  }
}
' "$raw_tsv" | sort -t "$(printf '\t')" -k2,2n -k5,5gr > "$summary_tsv"

{
  echo "DeltaNet isolated scan sweep"
  echo "metalpack: $metalpack"
  echo "raw: $raw_tsv"
  echo "summary: $summary_tsv"
  echo
  column -t -s "$(printf '\t')" "$summary_tsv"
} > "$summary_txt"

cat "$summary_txt"
