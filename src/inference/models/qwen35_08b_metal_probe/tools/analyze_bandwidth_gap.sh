#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/analyze_bandwidth_gap.sh --normalized normalized.txt [options]

Classifies a benchmark result against the byte/cache model without rerunning
benchmarks. This is an inferred roofline/gap report unless --actual-dram-bytes
is supplied from a hardware counter capture.

Required:
  --normalized FILE              normalized key/value output from tools/normalize_benchmark_output.sh

One byte model source is required:
  --modeled-bytes BYTES          modeled compulsory DRAM/miss-floor bytes for the measured scope
  --cache-csv FILE --op OP       cache_analysis --csv output plus op name

Options:
  --op OP                        operation name for report output
  --layer-scope model|single     multiply cache CSV row bytes by layers_per_model (default: model)
  --sustained-gb-s N             sustained bandwidth roofline assumption (default: 90)
  --actual-dram-bytes BYTES      measured GPU memory traffic from a hardware counter capture
  --warn-util FRACTION           bandwidth utilization warning threshold (default: 0.60)
  --warn-floor RATIO             time/floor warning threshold (default: 1.35)
  --markdown                     emit a compact markdown table

Output fields:
  bandwidth_utilization          effective_GB/s / sustained_GB/s
  time_vs_floor                  median_s / (modeled_bytes / sustained_Bps)
  traffic_vs_model               actual_dram_bytes / modeled_bytes, if hardware bytes are available
  avoidable_miss_suspect         yes only with counter-backed excess traffic; unverified otherwise
USAGE
}

normalized=""
cache_csv=""
op=""
modeled_bytes=""
actual_dram_bytes=""
layer_scope="model"
sustained_gb_s="90"
warn_util="0.60"
warn_floor="1.35"
markdown=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --normalized)
      normalized="${2:-}"
      shift 2
      ;;
    --cache-csv)
      cache_csv="${2:-}"
      shift 2
      ;;
    --op)
      op="${2:-}"
      shift 2
      ;;
    --modeled-bytes)
      modeled_bytes="${2:-}"
      shift 2
      ;;
    --actual-dram-bytes)
      actual_dram_bytes="${2:-}"
      shift 2
      ;;
    --layer-scope)
      layer_scope="${2:-}"
      shift 2
      ;;
    --sustained-gb-s)
      sustained_gb_s="${2:-}"
      shift 2
      ;;
    --warn-util)
      warn_util="${2:-}"
      shift 2
      ;;
    --warn-floor)
      warn_floor="${2:-}"
      shift 2
      ;;
    --markdown)
      markdown=1
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

if [[ -z "$normalized" || ! -f "$normalized" ]]; then
  echo "missing --normalized file: ${normalized:-<empty>}" >&2
  exit 2
fi

case "$layer_scope" in
  model|single) ;;
  *)
    echo "--layer-scope must be 'model' or 'single'" >&2
    exit 2
    ;;
esac

is_number() {
  [[ "$1" =~ ^[0-9]+([.][0-9]+)?$ ]]
}

field_value() {
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

median_s="$(field_value "$normalized" median_s)"
effective_gb_s="$(field_value "$normalized" effective_GB/s)"
tokens="$(field_value "$normalized" tokens/context)"
p95_s="$(field_value "$normalized" p95_s)"

if [[ -z "$median_s" || ! "$median_s" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  echo "normalized file is missing numeric median_s: $normalized" >&2
  exit 2
fi

cache_layers=""
cache_modeled_row_bytes=""
cache_logical_row_bytes=""
cache_hit_rate=""
cache_residency=""
cache_dominant=""
cache_kernel=""
cache_token_tile=""
cache_working_set_bytes=""

if [[ -n "$cache_csv" ]]; then
  if [[ -z "$op" ]]; then
    echo "--cache-csv requires --op" >&2
    exit 2
  fi
  if [[ ! -f "$cache_csv" ]]; then
    echo "missing --cache-csv file: $cache_csv" >&2
    exit 2
  fi
  cache_row="$(
    awk -F, -v op="$op" 'NR > 1 && $1 == op { print; exit }' "$cache_csv"
  )"
  if [[ -z "$cache_row" ]]; then
    echo "op not found in cache csv: $op" >&2
    exit 2
  fi
  IFS=, read -r _op cache_kernel cache_layers cache_token_tile cache_working_set_bytes cache_logical_row_bytes cache_modeled_row_bytes _budget cache_hit_bytes cache_hit_rate _modeled_time_ms cache_residency cache_dominant _optimization <<<"$cache_row"
  if [[ -z "$modeled_bytes" ]]; then
    if [[ "$layer_scope" == "model" ]]; then
      modeled_bytes="$(awk -v row="$cache_modeled_row_bytes" -v layers="$cache_layers" 'BEGIN { printf "%.0f", row * layers }')"
    else
      modeled_bytes="$cache_modeled_row_bytes"
    fi
  fi
fi

if [[ -z "$modeled_bytes" || ! "$modeled_bytes" =~ ^[0-9]+$ ]]; then
  echo "missing numeric modeled bytes; pass --modeled-bytes or --cache-csv --op" >&2
  exit 2
fi

if [[ -n "$actual_dram_bytes" && ! "$actual_dram_bytes" =~ ^[0-9]+$ ]]; then
  echo "--actual-dram-bytes must be an integer byte count" >&2
  exit 2
fi

if [[ -z "$effective_gb_s" ]]; then
  effective_gb_s="$(awk -v bytes="$modeled_bytes" -v median="$median_s" 'BEGIN { if (median > 0) printf "%.6f", bytes / median / 1e9 }')"
fi

if ! is_number "$effective_gb_s" || ! is_number "$sustained_gb_s" || ! is_number "$warn_util" || ! is_number "$warn_floor"; then
  echo "effective_GB/s, sustained-gb-s, warn-util, and warn-floor must be numeric" >&2
  exit 2
fi

report="$(
  awk \
    -v op="${op:-n/a}" \
    -v tokens="${tokens:-n/a}" \
    -v median="$median_s" \
    -v p95="${p95_s:-n/a}" \
    -v eff="$effective_gb_s" \
    -v sustained="$sustained_gb_s" \
    -v modeled="$modeled_bytes" \
    -v actual="${actual_dram_bytes:-}" \
    -v warn_util="$warn_util" \
    -v warn_floor="$warn_floor" \
    -v cache_layers="${cache_layers:-n/a}" \
    -v cache_kernel="${cache_kernel:-n/a}" \
    -v cache_tile="${cache_token_tile:-n/a}" \
    -v cache_working="${cache_working_set_bytes:-n/a}" \
    -v cache_hit_rate="${cache_hit_rate:-n/a}" \
    -v cache_residency="${cache_residency:-n/a}" \
    -v cache_dominant="${cache_dominant:-n/a}" \
    -v markdown="$markdown" '
  function yn(v) { return v ? "yes" : "no" }
  BEGIN {
    roof_s = modeled / (sustained * 1e9)
    time_vs_floor = median / (roof_s > 0 ? roof_s : 1e-12)
    util = eff / sustained
    dram_equiv = median * sustained * 1e9
    reported_bytes = eff * 1e9 * median
    reported_vs_model = reported_bytes / (modeled > 0 ? modeled : 1)
    byte_model_mismatch = reported_vs_model < 0.85 || reported_vs_model > 1.15
    traffic_vs_model = "n/a"
    avoidable = "unverified"
    has_actual = actual != ""
    if (has_actual) {
      traffic_ratio = actual / (modeled > 0 ? modeled : 1)
      traffic_vs_model = sprintf("%.3f", traffic_ratio)
      avoidable = traffic_ratio > 1.15 ? "yes" : "no"
    } else if (time_vs_floor <= warn_floor) {
      avoidable = "no-counter-near-floor"
    }

    bw_gap = util < warn_util
    floor_gap = time_vs_floor > warn_floor
    occupancy = (floor_gap && !has_actual) ? "yes" : "no"

    if (byte_model_mismatch) {
      class = "byte-model-mismatch"
      next_probe = "align benchmark effective_GB/s bytes with forensics modeled bytes before using roofline conclusions"
    } else if (has_actual && traffic_ratio > 1.15) {
      class = "avoidable-memory-traffic"
      next_probe = "compare trace bytes to byte buckets; remove rereads/scratch or change layout"
    } else if (floor_gap && bw_gap) {
      class = "bandwidth-underutilization"
      next_probe = "profile tile occupancy, tail underfill, scratch, dispatch boundaries"
    } else if (floor_gap) {
      class = "roofline-assumption-or-byte-model-gap"
      next_probe = "calibrate sustained bandwidth for this shape and audit modeled bytes"
    } else {
      class = "near-modeled-floor"
      next_probe = "attack algorithmic bytes: larger token tile, fusion, quantization, or fewer passes"
    }

    if (markdown == 1) {
      print "| op | median_s | effective_GB/s | util | modeled_bytes | reported/model | floor_s | time/floor | traffic/model | class |"
      print "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |"
      printf "| %s | %.9f | %.3f | %.1f%% | %.0f | %.3f | %.9f | %.2fx | %s | %s |\n", op, median, eff, util * 100.0, modeled, reported_vs_model, roof_s, time_vs_floor, traffic_vs_model, class
      exit
    }

    print "op: " op
    print "tokens/context: " tokens
    print "median_s: " sprintf("%.9f", median)
    print "p95_s: " p95
    print "effective_GB/s: " sprintf("%.6f", eff)
    print "sustained_GB/s: " sprintf("%.6f", sustained)
    print "bandwidth_utilization: " sprintf("%.6f", util)
    print "modeled_dram_miss_bytes: " sprintf("%.0f", modeled)
    print "reported_effective_implied_bytes: " sprintf("%.0f", reported_bytes)
    print "reported_effective_vs_modeled: " sprintf("%.6f", reported_vs_model)
    print "byte_model_mismatch_suspect: " yn(byte_model_mismatch)
    print "roofline_floor_s: " sprintf("%.9f", roof_s)
    print "time_vs_floor: " sprintf("%.6f", time_vs_floor)
    print "dram_equiv_bytes_at_sustained: " sprintf("%.0f", dram_equiv)
    print "actual_dram_bytes: " (has_actual ? actual : "n/a")
    print "traffic_vs_model: " traffic_vs_model
    print "bandwidth_gap_suspect: " yn(bw_gap)
    print "floor_gap_suspect: " yn(floor_gap)
    print "avoidable_miss_suspect: " avoidable
    print "occupancy_or_dispatch_suspect: " occupancy
    print "classification: " class
    print "next_probe: " next_probe
    print "cache_counter_status: " (has_actual ? "hardware-counter-backed" : "inferred-only")
    print "cache_kernel: " cache_kernel
    print "cache_layers: " cache_layers
    print "cache_token_tile: " cache_tile
    print "cache_working_set_bytes: " cache_working
    print "cache_modeled_hit_rate: " cache_hit_rate
    print "cache_residency: " cache_residency
    print "cache_dominant: " cache_dominant
  }'
)"

printf '%s\n' "$report"
