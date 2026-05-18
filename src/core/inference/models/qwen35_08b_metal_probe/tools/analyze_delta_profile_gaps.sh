#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/analyze_delta_profile_gaps.sh <profile-stdout.txt> <cache-analysis.csv> [--sustained-gb-s N] [--markdown]

Combines profile_metalpack_prefill_delta_stack prefix timings with
cache_analysis --csv modeled bytes to classify DeltaNet+FFN phases.
This does not run benchmarks.
USAGE
}

if [[ $# -lt 2 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

profile="$1"
cache_csv="$2"
shift 2

if [[ ! -f "$profile" ]]; then
  echo "missing profile stdout: $profile" >&2
  exit 2
fi
if [[ ! -f "$cache_csv" ]]; then
  echo "missing cache analysis csv: $cache_csv" >&2
  exit 2
fi

sustained_gb_s="90"
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

awk -F, -v profile="$profile" -v sustained="$sustained_gb_s" -v markdown="$markdown" '
function add_phase(phase, op_name,     i, found) {
  found = 0
  for (i = 1; i <= nops; i++) {
    if (op[i] == op_name) {
      phase_bytes[phase] += modeled[i] * layers[i]
      phase_logical[phase] += logical[i] * layers[i]
      phase_ops[phase] = (phase_ops[phase] == "" ? op_name : phase_ops[phase] "+" op_name)
      found = 1
      break
    }
  }
  if (!found) missing[phase] = missing[phase] == "" ? op_name : missing[phase] "+" op_name
}
function class(util, ratio) {
  if (util > 1.15) return "byte-model-overcount-or-roof-mismatch"
  if (ratio > 1.35 && util < 0.60) return "bandwidth-underutilization"
  if (ratio > 1.35) return "roofline-or-byte-model-gap"
  return "near-modeled-floor"
}
function next_probe(phase, c) {
  if (c == "byte-model-overcount-or-roof-mismatch") return "audit phase parser and byte bucket before using this row for optimization"
  if (c == "bandwidth-underutilization") return "inspect occupancy/tile/register pressure/dispatch fragmentation"
  if (c == "roofline-or-byte-model-gap") return "audit phase byte buckets and calibrate roof"
  if (phase == "attention.core") return "reduce algorithmic bytes"
  return "reduce algorithmic traffic or leave as lower priority"
}
BEGIN {
  while ((getline line < profile) > 0) {
    if (line ~ /^shape:/) {
      shape = line
    }
    if (line ~ /^project[[:space:]]/) {
      split(line, a, /[ \t]+/)
      phase_ms["project"] = a[3] + 0.0
      order[++nphase] = "project"
    } else if (line ~ /^conv\/split\+ba[[:space:]]/) {
      split(line, a, /[ \t]+/)
      phase_ms["conv/split+ba"] = a[3] + 0.0
      order[++nphase] = "conv/split+ba"
    } else if (line ~ /^scan\+norm[[:space:]]/) {
      split(line, a, /[ \t]+/)
      phase_ms["scan+norm"] = a[3] + 0.0
      order[++nphase] = "scan+norm"
    } else if (line ~ /^delta out[[:space:]]/) {
      split(line, a, /[ \t]+/)
      phase_ms["delta out"] = a[4] + 0.0
      order[++nphase] = "delta out"
    } else if (line ~ /^ffn norm\+gate\/up[[:space:]]/) {
      split(line, a, /[ \t]+/)
      phase_ms["ffn norm+gate/up"] = a[4] + 0.0
      order[++nphase] = "ffn norm+gate/up"
    } else if (line ~ /^ffn down[[:space:]]/) {
      split(line, a, /[ \t]+/)
      phase_ms["ffn down"] = a[4] + 0.0
      order[++nphase] = "ffn down"
    }
  }
}
NR > 1 {
  nops++
  op[nops] = $1
  layers[nops] = $3 + 0
  logical[nops] = $6 + 0
  modeled[nops] = $7 + 0
}
END {
  add_phase("project", "delta.project.qkv")
  add_phase("project", "delta.project.z")
  add_phase("conv/split+ba", "delta.project.b/a")
  add_phase("conv/split+ba", "delta.conv1d")
  add_phase("conv/split+ba", "delta.prepare.qkv_beta_decay")
  add_phase("scan+norm", "delta.recurrent_scan")
  add_phase("scan+norm", "delta.gated_rmsnorm")
  add_phase("delta out", "delta.out_proj")
  add_phase("ffn norm+gate/up", "ffn.gate_up_swiglu")
  add_phase("ffn down", "ffn.down")

  if (markdown == 1) {
    print "| phase | delta_ms | modeled_bytes | eff_GB/s | util | floor_ms | time/floor | class | next_probe |"
    print "| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |"
  } else {
    print "source_profile: " profile
    print "source_cache_csv: " FILENAME
    print "sustained_GB/s: " sustained
    print "shape: " shape
  }
  worst = ""
  worst_ratio = 0
  for (i = 1; i <= nphase; i++) {
    p = order[i]
    ms = phase_ms[p]
    bytes = phase_bytes[p]
    if (ms <= 0 || bytes <= 0) {
      if (markdown == 1) {
        printf "| %s | %.3f | %.0f | n/a | n/a | n/a | n/a | incomplete-profile-or-byte-model | fix parser/model coverage |\n", p, ms, bytes
      } else {
        print ""
        print "phase: " p
        print "delta_ms: " sprintf("%.3f", ms)
        print "modeled_dram_miss_bytes: " sprintf("%.0f", bytes)
        print "classification: incomplete-profile-or-byte-model"
      }
      continue
    }
    eff = bytes / (ms / 1000.0) / 1e9
    util = eff / sustained
    floor_ms = bytes / (sustained * 1e9) * 1000.0
    ratio = ms / (floor_ms > 0 ? floor_ms : 1e-12)
    c = class(util, ratio)
    np = next_probe(p, c)
    if (ratio > worst_ratio) {
      worst_ratio = ratio
      worst = p
    }
    if (markdown == 1) {
      printf "| %s | %.3f | %.0f | %.2f | %.1f%% | %.3f | %.2fx | %s | %s |\n", p, ms, bytes, eff, util * 100.0, floor_ms, ratio, c, np
    } else {
      print ""
      print "phase: " p
      print "delta_ms: " sprintf("%.3f", ms)
      print "modeled_dram_miss_bytes: " sprintf("%.0f", bytes)
      print "effective_GB/s: " sprintf("%.6f", eff)
      print "bandwidth_utilization: " sprintf("%.6f", util)
      print "roofline_floor_ms: " sprintf("%.3f", floor_ms)
      print "time_vs_floor: " sprintf("%.6f", ratio)
      print "classification: " c
      print "phase_ops: " phase_ops[p]
      print "next_probe: " np
    }
  }
  if (markdown != 1) {
    print ""
    print "worst_phase_by_time_vs_floor: " worst
    print "worst_time_vs_floor: " sprintf("%.6f", worst_ratio)
  }
}
' "$cache_csv"
