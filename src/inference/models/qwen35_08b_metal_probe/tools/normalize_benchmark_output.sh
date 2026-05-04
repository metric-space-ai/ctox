#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/normalize_benchmark_output.sh <benchmark-output.txt>

Parses existing benchmark stdout into a normalized key/value evidence block.
This script does not run benchmarks.
USAGE
}

if [[ $# -ne 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

input="$1"
if [[ ! -f "$input" ]]; then
  echo "missing benchmark output: $input" >&2
  exit 2
fi

awk -v source="$input" '
function trim(s) {
  sub(/^[ \t\r\n]+/, "", s)
  sub(/[ \t\r\n]+$/, "", s)
  return s
}

function set_once(key, value) {
  value = trim(value)
  if (value != "" && !(key in data)) {
    data[key] = value
  }
}

function set_always(key, value) {
  value = trim(value)
  if (value != "") {
    data[key] = value
  }
}

function append_env(kind, value) {
  value = trim(value)
  if (value == "") {
    return
  }
  if (kind == "best") {
    best_env = best_env == "" ? value : best_env "; " value
  } else {
    accepted_env = accepted_env == "" ? value : accepted_env "; " value
  }
}

function parse_equals_metric(line, key, out_key, value) {
  value = line
  if (index(value, key "=") == 0) {
    return
  }
  sub("^.*" key "=", "", value)
  sub("[^0-9.].*$", "", value)
  set_once(out_key, value)
}

BEGIN {
  in_best_env = 0
  in_accepted_env = 0
  best_env = ""
  accepted_env = ""
  print "source_output: " source
}

{
  raw = $0
  line = trim(raw)

  if (line == "best_env:") {
    in_best_env = 1
    in_accepted_env = 0
    next
  }
  if (line == "accepted_env:") {
    in_best_env = 0
    in_accepted_env = 1
    next
  }

  if (in_best_env || in_accepted_env) {
    if (raw ~ /^[ \t]+[A-Za-z_][A-Za-z0-9_]*=/) {
      append_env(in_best_env ? "best" : "accepted", raw)
      next
    }
    if (line == "") {
      next
    }
    in_best_env = 0
    in_accepted_env = 0
  }

  if (match(line, /shape:.*tokens=[0-9]+/)) {
    shape = line
    sub(/^.*tokens=/, "", shape)
    sub(/[^0-9].*$/, "", shape)
    set_once("tokens/context", shape)
  }
  if (match(line, /shape:.*iterations=[0-9]+/)) {
    shape = line
    sub(/^.*iterations=/, "", shape)
    sub(/[^0-9].*$/, "", shape)
    set_once("iterations", shape)
  }
  if (match(line, /shape:.*warmup=[0-9]+/)) {
    shape = line
    sub(/^.*warmup=/, "", shape)
    sub(/[^0-9].*$/, "", shape)
    set_once("warmup", shape)
  }

  if (index(line, ":") > 0) {
    split(line, parts, ":")
    key = trim(parts[1])
    value = substr(line, index(line, ":") + 1)
    value = trim(value)

    if (key == "tokens") {
      set_once("tokens/context", value)
    } else if (key == "iterations") {
      set_once("iterations", value)
    } else if (key == "median_s") {
      set_once("median_s", value)
    } else if (key == "p95_s") {
      set_once("p95_s", value)
    } else if (key == "full_prefill_estimate_current_kernels") {
      full_prefill = value
      sub(/s,.*$/, "", full_prefill)
      set_once("median_s", full_prefill)
    } else if (key == "best_selection") {
      set_always("best_selection", value)
    } else if (key == "accepted_selection") {
      set_always("accepted_selection", value)
    } else if (key == "best_median_s") {
      set_always("best_median_s", value)
    } else if (key == "best_p95_s") {
      set_always("best_p95_s", value)
    } else if (key ~ /^best_tok_s/) {
      set_always("best_tok_s", value)
    } else if (key == "evaluations") {
      set_always("candidate_count", value)
    } else if (key == "history_csv" || key == "summary_csv") {
      set_always("output_csv", value)
    } else if (key == "correctness_gate") {
      set_always("correctness_gate", value)
    } else if (key ~ /^checksum/) {
      set_once("checksum", value)
    } else if (tolower(key) ~ /effective/ && tolower(key) ~ /gb/) {
      set_once("effective_GB/s", value)
    }
  }

  if (line ~ /^baseline:/) {
    parse_equals_metric(line, "median", "baseline_median_s")
    parse_equals_metric(line, "p95", "baseline_p95_s")
    parse_equals_metric(line, "eff", "baseline_effective_GB/s")
  }
}

END {
  if (best_env != "") {
    data["best_env"] = best_env
  }
  if (accepted_env != "") {
    data["accepted_env"] = accepted_env
  }
  if (!("p95_s" in data) && ("median_s" in data) && ("iterations" in data) && data["iterations"] == "1") {
    data["p95_s"] = data["median_s"]
  }

  order[1] = "tokens/context"
  order[2] = "iterations"
  order[3] = "warmup"
  order[4] = "median_s"
  order[5] = "p95_s"
  order[6] = "effective_GB/s"
  order[7] = "checksum"
  order[8] = "baseline_median_s"
  order[9] = "baseline_p95_s"
  order[10] = "baseline_effective_GB/s"
  order[11] = "best_selection"
  order[12] = "accepted_selection"
  order[13] = "best_env"
  order[14] = "accepted_env"
  order[15] = "best_median_s"
  order[16] = "best_p95_s"
  order[17] = "best_tok_s"
  order[18] = "candidate_count"
  order[19] = "correctness_gate"
  order[20] = "output_csv"

  for (i = 1; i <= 20; i++) {
    key = order[i]
    if (key in data) {
      print key ": " data[key]
    }
  }
}
' "$input"
