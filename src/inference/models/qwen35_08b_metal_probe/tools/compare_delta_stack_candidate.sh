#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/compare_delta_stack_candidate.sh --candidate-env KEY=VALUE [options]

Alternates accepted-profile and candidate DeltaNet+FFN stack runs to reduce
order/thermal bias when judging small kernel wins.

Options:
  --metalpack PATH        default: /tmp/ctox_qwen35_08b_real_fp16.metalpack
  --tokens N[,N...]      default: 4096
  --rounds N             default: 3
  --iterations N         default: 3
  --warmup N             default: 1
  --layers N             default: 18
  --start-layer N        default: 0
  --mps-ffn-sidecar DIR  optional MPS FFN sidecar passed to the stack bench
  --mps-delta-project-sidecar DIR
                          optional MPS Delta project sidecar
  --mps-delta-out-sidecar DIR
                          optional MPS Delta out sidecar
  --candidate-reset-tuning-env
                          source accepted profile, unset mutually-exclusive
                          Delta stack tuning flags, then apply candidate envs
  --candidate-env K=V    may be repeated
  --output-dir DIR       default: /tmp/ctox_qwen35_paired_<pid>
USAGE
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
metalpack="/tmp/ctox_qwen35_08b_real_fp16.metalpack"
tokens=4096
rounds=3
iterations=3
warmup=1
layers=18
start_layer=0
output_dir=""
mps_ffn_sidecar=""
mps_delta_project_sidecar=""
mps_delta_out_sidecar=""
candidate_reset_tuning_env=0
candidate_envs=()

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
    --layers)
      layers="${2:-}"
      shift 2
      ;;
    --start-layer)
      start_layer="${2:-}"
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
    --candidate-reset-tuning-env)
      candidate_reset_tuning_env=1
      shift
      ;;
    --candidate-env)
      candidate_envs+=("${2:-}")
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

if [[ "${#candidate_envs[@]}" -eq 0 ]]; then
  echo "at least one --candidate-env KEY=VALUE is required" >&2
  usage
  exit 2
fi
if [[ ! -d "$metalpack" ]]; then
  echo "missing metalpack directory: $metalpack" >&2
  exit 2
fi
if [[ -n "$mps_delta_project_sidecar" && -z "$mps_ffn_sidecar" ]]; then
  echo "--mps-delta-project-sidecar requires --mps-ffn-sidecar because the bench arguments are positional" >&2
  exit 2
fi
if [[ -n "$mps_delta_out_sidecar" && ( -z "$mps_ffn_sidecar" || -z "$mps_delta_project_sidecar" ) ]]; then
  echo "--mps-delta-out-sidecar requires both preceding MPS sidecars because the bench arguments are positional" >&2
  exit 2
fi
for sidecar in "$mps_ffn_sidecar" "$mps_delta_project_sidecar" "$mps_delta_out_sidecar"; do
  if [[ -n "$sidecar" && ! -d "$sidecar" ]]; then
    echo "missing sidecar directory: $sidecar" >&2
    exit 2
  fi
done

output_dir="${output_dir:-/tmp/ctox_qwen35_paired_$$}"
mkdir -p "$output_dir"

if [[ "$tokens" == *,* ]]; then
  IFS=',' read -r -a token_values <<< "$tokens"
  combined="$output_dir/combined_results.txt"
  : > "$combined"
  for token_value in "${token_values[@]}"; do
    if [[ ! "$token_value" =~ ^[0-9]+$ ]]; then
      echo "invalid tokens value in list: $token_value" >&2
      exit 2
    fi
    subdir="$output_dir/tokens_$token_value"
    cmd=(
      "$0"
      --metalpack "$metalpack"
      --tokens "$token_value"
      --rounds "$rounds"
      --iterations "$iterations"
      --warmup "$warmup"
      --layers "$layers"
      --start-layer "$start_layer"
      --output-dir "$subdir"
    )
    if [[ -n "$mps_ffn_sidecar" ]]; then
      cmd+=(--mps-ffn-sidecar "$mps_ffn_sidecar")
    fi
    if [[ -n "$mps_delta_project_sidecar" ]]; then
      cmd+=(--mps-delta-project-sidecar "$mps_delta_project_sidecar")
    fi
    if [[ -n "$mps_delta_out_sidecar" ]]; then
      cmd+=(--mps-delta-out-sidecar "$mps_delta_out_sidecar")
    fi
    if [[ "$candidate_reset_tuning_env" -eq 1 ]]; then
      cmd+=(--candidate-reset-tuning-env)
    fi
    for candidate_env in "${candidate_envs[@]}"; do
      cmd+=(--candidate-env "$candidate_env")
    done
    echo "== tokens $token_value ==" | tee -a "$combined"
    "${cmd[@]}" | tee -a "$combined"
  done
  echo "combined: $combined"
  exit 0
fi

if [[ ! "$tokens" =~ ^[0-9]+$ ]]; then
  echo "invalid tokens argument \`$tokens\`: expected integer or comma-separated integers" >&2
  exit 2
fi

bench="$repo_root/target/release/bench_metalpack_prefill_delta3_ffn_superblock"
if [[ ! -x "$bench" ]]; then
  echo "missing bench binary: $bench" >&2
  echo "run: cargo build --release --bin bench_metalpack_prefill_delta3_ffn_superblock" >&2
  exit 2
fi

tuning_env_keys=(
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA8
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA16
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA32
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA64
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_MMA128_RG4_ASHARED
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_NO_MMA
  CTOX_QWEN35_DELTA_OUT_MMA16
  CTOX_QWEN35_DELTA_OUT_MMA32
  CTOX_QWEN35_DELTA_OUT_MMA32_RESIDUAL
  CTOX_QWEN35_DELTA_OUT_MMA64
  CTOX_QWEN35_DELTA_OUT_TOK2
  CTOX_QWEN35_DELTA_OUT_TOK8
  CTOX_QWEN35_FFN_GATE_UP_MMA
  CTOX_QWEN35_FFN_GATE_UP_MMA16
  CTOX_QWEN35_FFN_GATE_UP_MMA32
  CTOX_QWEN35_FFN_GATE_UP_MMA64
  CTOX_QWEN35_FFN_GATE_UP_TOK2
  CTOX_QWEN35_FFN_GATE_UP_TOK8
  CTOX_QWEN35_DOWN_MMA
  CTOX_QWEN35_DOWN_MMA16
  CTOX_QWEN35_DOWN_MMA32
  CTOX_QWEN35_DOWN_MMA32_RESIDUAL
  CTOX_QWEN35_DOWN_MMA64
  CTOX_QWEN35_DOWN_MMA64_RESIDUAL
  CTOX_QWEN35_DOWN_TOK2
  CTOX_QWEN35_DOWN_TOK8
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
  CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED
  CTOX_QWEN35_DELTA_CONV_SPLIT_FUSED_TOK4
)

run_case() {
  local label="$1"
  local path="$2"
  shift 2
  local bench_args=(
    "$metalpack"
    "$start_layer"
    "$tokens"
    "$iterations"
    "$warmup"
    "$layers"
  )
  if [[ -n "$mps_ffn_sidecar" ]]; then
    bench_args+=("$mps_ffn_sidecar")
  fi
  if [[ -n "$mps_delta_project_sidecar" ]]; then
    bench_args+=("$mps_delta_project_sidecar")
  fi
  if [[ -n "$mps_delta_out_sidecar" ]]; then
    bench_args+=("$mps_delta_out_sidecar")
  fi
  if [[ "$label" == "candidate" ]]; then
    if [[ "$candidate_reset_tuning_env" -eq 1 ]]; then
      (
        # shellcheck source=/dev/null
        source "$repo_root/docs/kernel-dev/accepted_profile.env"
        for key in "${tuning_env_keys[@]}"; do
          unset "$key"
        done
        for assignment in "$@"; do
          export "$assignment"
        done
        "$bench" "${bench_args[@]}"
      ) > "$path"
    else
      env "$@" "$repo_root/tools/run_accepted_profile.sh" "$bench" "${bench_args[@]}" > "$path"
    fi
  else
    "$repo_root/tools/run_accepted_profile.sh" "$bench" "${bench_args[@]}" > "$path"
  fi
}

extract_metric() {
  local key="$1"
  local file="$2"
  awk -v key="$key" '
    index($0, key ":") == 1 {
      sub(/^[^:]*:[ \t]*/, "")
      print
      exit
    }
  ' "$file"
}

csv="$output_dir/paired_results.csv"
echo "round,order,label,median_s,p95_s,checksum16,path" > "$csv"

for ((round = 1; round <= rounds; round++)); do
  if (( round % 2 == 1 )); then
    order=(baseline candidate)
  else
    order=(candidate baseline)
  fi
  order_label="${order[0]}-${order[1]}"
  for label in "${order[@]}"; do
    path="$output_dir/round_${round}_${label}.txt"
    if [[ "$label" == "candidate" ]]; then
      run_case "$label" "$path" "${candidate_envs[@]}"
    else
      run_case "$label" "$path"
    fi
    median="$(extract_metric median_s "$path")"
    p95="$(extract_metric p95_s "$path")"
    checksum="$(extract_metric checksum16 "$path")"
    echo "$round,$order_label,$label,$median,$p95,$checksum,$path" >> "$csv"
  done
done

awk -F, -v tokens="$tokens" -v csv="$csv" '
NR > 1 {
  key = $3
  median[key, ++n[key]] = $4 + 0
  p95[key, n[key]] = $5 + 0
  checksum[key] = $6
}
function sort_values(arr, count,    i, j, tmp) {
  for (i = 1; i <= count; i++) {
    for (j = i + 1; j <= count; j++) {
      if (arr[i] > arr[j]) {
        tmp = arr[i]; arr[i] = arr[j]; arr[j] = tmp
      }
    }
  }
}
function median_of(arr, count) {
  sort_values(arr, count)
  if (count % 2 == 1) return arr[(count + 1) / 2]
  return (arr[count / 2] + arr[count / 2 + 1]) / 2.0
}
END {
  for (i = 1; i <= n["baseline"]; i++) {
    base_m[i] = median["baseline", i]
    base_p[i] = p95["baseline", i]
  }
  for (i = 1; i <= n["candidate"]; i++) {
    cand_m[i] = median["candidate", i]
    cand_p[i] = p95["candidate", i]
  }
  bm = median_of(base_m, n["baseline"])
  cm = median_of(cand_m, n["candidate"])
  bp = median_of(base_p, n["baseline"])
  cp = median_of(cand_p, n["candidate"])
  printf "paired_delta_stack_compare\n"
  printf "tokens: %s\n", tokens
  printf "rounds: %d\n", n["baseline"]
  printf "csv: %s\n", csv
  printf "baseline_median_s: %.9f\n", bm
  printf "candidate_median_s: %.9f\n", cm
  printf "median_delta_percent: %.4f\n", (cm / bm - 1.0) * 100.0
  printf "baseline_p95_s: %.9f\n", bp
  printf "candidate_p95_s: %.9f\n", cp
  printf "p95_delta_percent: %.4f\n", (cp / bp - 1.0) * 100.0
  printf "baseline_checksum16: %s\n", checksum["baseline"]
  printf "candidate_checksum16: %s\n", checksum["candidate"]
}
' "$csv"
