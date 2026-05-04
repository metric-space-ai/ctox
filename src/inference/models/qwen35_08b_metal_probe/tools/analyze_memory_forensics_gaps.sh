#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/analyze_memory_forensics_gaps.sh <memory-forensics-stdout.txt> [--sustained-gb-s N] [--markdown]

Parses target/release/memory_forensics output and ranks per-scope roofline gaps.
This does not run benchmarks. Cache/miss conclusions are inferred unless the
input file itself names hardware cache/memory counters.
USAGE
}

if [[ $# -lt 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

input="$1"
shift

if [[ ! -f "$input" ]]; then
  echo "missing memory forensics output: $input" >&2
  exit 2
fi

sustained_gb_s=""
markdown=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sustained-gb-s)
      sustained_gb_s="${2:-}"
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

awk -v sustained_arg="$sustained_gb_s" -v markdown="$markdown" '
function unit_bytes(value, unit) {
  if (unit == "GiB") return value * 1024 * 1024 * 1024
  if (unit == "MiB") return value * 1024 * 1024
  if (unit == "KiB") return value * 1024
  return value
}

function classification(util, ratio, eff_vs_model, has_counter) {
  if (eff_vs_model < 0.85 || eff_vs_model > 1.15) return "byte-model-mismatch"
  if (ratio > warn_floor && util < warn_util) return "bandwidth-underutilization"
  if (ratio > warn_floor) return "roofline-or-byte-model-gap"
  return "near-modeled-floor"
}

function next_probe_for(class, scope) {
  if (class == "byte-model-mismatch") return "align benchmark bytes with forensics model"
  if (class == "bandwidth-underutilization") return "inspect occupancy, tail underfill, scratch, dispatch boundaries"
  if (class == "roofline-or-byte-model-gap") return "calibrate sustained bandwidth and audit byte buckets"
  if (scope == "delta18+ffn") return "reduce algorithmic bytes: chunked/fused DeltaNet+FFN prefill"
  if (scope == "attention.core") return "reduce T^2 KV bytes: FlashAttention-style tiling or compressed/block attention"
  return "attack algorithmic bytes before micro-tuning"
}

BEGIN {
  warn_util = 0.60
  warn_floor = 1.35
  sustained = sustained_arg
}

/^sustained_bandwidth_assumption:/ {
  if (sustained == "") sustained = $2
}

/^counter_limit:/ {
  counter_status = "inferred-only"
}

/^full_prefill_estimate_current_kernels:/ {
  full_prefill = $2
  gsub(/s,/, "", full_prefill)
  full_tok_s = $3
}

($1 == "delta18+ffn" || $1 == "attention.core" || $1 == "attention.ffn") {
  scope[++n] = $1
  median_ms[n] = $2 + 0.0
  eff_gb_s[n] = $3 + 0.0
  model_bytes[n] = unit_bytes($4 + 0.0, $5)
  floor_ms[n] = $6 + 0.0
  ratio = $9
  gsub(/x/, "", ratio)
  time_ratio[n] = ratio + 0.0
}

END {
  if (sustained == "") sustained = 90.0
  if (counter_status == "") counter_status = "inferred-only"

  if (markdown == 1) {
    print "| scope | median_ms | eff_GB/s | util | model_bytes | floor_ms | time/floor | class | next_probe |"
    print "| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |"
  } else {
    print "source_output: " FILENAME
    print "sustained_GB/s: " sustained
    print "cache_counter_status: " counter_status
    if (full_prefill != "") {
      print "full_prefill_estimate_s: " full_prefill
      print "full_prefill_tok_s: " full_tok_s
    }
  }

  worst_idx = 0
  worst_ratio = 0.0
  for (i = 1; i <= n; i++) {
    util = eff_gb_s[i] / sustained
    reported_bytes = eff_gb_s[i] * 1e9 * median_ms[i] / 1000.0
    eff_vs_model = reported_bytes / model_bytes[i]
    class = classification(util, time_ratio[i], eff_vs_model, counter_status != "inferred-only")
    next_probe = next_probe_for(class, scope[i])
    if (time_ratio[i] > worst_ratio) {
      worst_ratio = time_ratio[i]
      worst_idx = i
    }
    if (markdown == 1) {
      printf "| %s | %.3f | %.2f | %.1f%% | %.0f | %.3f | %.2fx | %s | %s |\n", scope[i], median_ms[i], eff_gb_s[i], util * 100.0, model_bytes[i], floor_ms[i], time_ratio[i], class, next_probe
    } else {
      print ""
      print "scope: " scope[i]
      print "median_ms: " sprintf("%.3f", median_ms[i])
      print "effective_GB/s: " sprintf("%.6f", eff_gb_s[i])
      print "bandwidth_utilization: " sprintf("%.6f", util)
      print "modeled_dram_miss_bytes: " sprintf("%.0f", model_bytes[i])
      print "roofline_floor_ms: " sprintf("%.3f", floor_ms[i])
      print "time_vs_floor: " sprintf("%.6f", time_ratio[i])
      print "reported_effective_vs_modeled: " sprintf("%.6f", eff_vs_model)
      print "classification: " class
      print "next_probe: " next_probe
    }
  }
  if (markdown == 0 && worst_idx > 0) {
    print ""
    print "worst_scope_by_time_vs_floor: " scope[worst_idx]
    print "worst_time_vs_floor: " sprintf("%.6f", worst_ratio)
  }
}
' "$input"
