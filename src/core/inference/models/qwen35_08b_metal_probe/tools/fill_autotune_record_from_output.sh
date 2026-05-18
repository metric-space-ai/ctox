#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/fill_autotune_record_from_output.sh <autotune-output.txt> <autotune-record.md>

Fills extractable fields in an autotune record from existing autotuner stdout.
It does not run benchmarks and does not fill correctness/error fields that are
not present in the output.
USAGE
}

if [[ $# -ne 2 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

output="$1"
record="$2"

if [[ ! -f "$output" ]]; then
  echo "missing autotune output: $output" >&2
  exit 2
fi
if [[ ! -f "$record" ]]; then
  echo "missing autotune record: $record" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

normalized="$(mktemp /tmp/ctox_autotune_normalized.XXXXXX)"
tools/normalize_benchmark_output.sh "$output" > "$normalized"

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

replace_field_if_present() {
  local source_key="$1"
  local target_key="$2"
  local value
  value="$(field_value "$normalized" "$source_key")"
  if [[ -z "$value" ]]; then
    return
  fi
  KEY="$target_key" VALUE="$value" perl -0pi -e '
    my $key = $ENV{"KEY"};
    my $value = $ENV{"VALUE"};
    s/^(\Q$key\E:).*$/$1 $value/m;
  ' "$record"
}

replace_literal() {
  local target_key="$1"
  local value="$2"
  KEY="$target_key" VALUE="$value" perl -0pi -e '
    my $key = $ENV{"KEY"};
    my $value = $ENV{"VALUE"};
    s/^(\Q$key\E:).*$/$1 $value/m;
  ' "$record"
}

replace_field_if_present output_csv output_csv
replace_field_if_present tokens/context tokens/context
replace_field_if_present iterations iterations
replace_field_if_present warmup warmup
replace_field_if_present candidate_count candidate_count
replace_field_if_present baseline_median_s baseline_median_s
replace_field_if_present baseline_p95_s baseline_p95_s
replace_field_if_present best_selection best_selection
replace_field_if_present best_median_s best_median_s
replace_field_if_present best_p95_s best_p95_s
replace_field_if_present best_tok_s best_tok_s
replace_field_if_present correctness_gate correctness_gate

accepted_env="$(field_value "$normalized" accepted_env)"
best_env="$(field_value "$normalized" best_env)"
if [[ -n "$accepted_env" ]]; then
  replace_literal chosen_env "$accepted_env"
elif [[ -n "$best_env" ]]; then
  replace_literal chosen_env "$best_env"
fi

baseline_median="$(field_value "$normalized" baseline_median_s)"
tokens="$(field_value "$normalized" tokens/context)"
if [[ -n "$baseline_median" && -n "$tokens" ]]; then
  baseline_tok_s="$(awk -v tokens="$tokens" -v median="$baseline_median" 'BEGIN { if (median > 0) printf "%.6f", tokens / median }')"
  replace_literal baseline_tok_s "$baseline_tok_s"
fi

best_median="$(field_value "$normalized" best_median_s)"
best_p95="$(field_value "$normalized" best_p95_s)"
baseline_p95="$(field_value "$normalized" baseline_p95_s)"
if [[ -n "$baseline_median" && -n "$best_median" ]]; then
  median_delta="$(awk -v base="$baseline_median" -v best="$best_median" 'BEGIN { if (base > 0) printf "%.6f", (best - base) / base * 100.0 }')"
  replace_literal median_delta_percent "$median_delta"
fi
if [[ -n "$baseline_p95" && -n "$best_p95" ]]; then
  p95_delta="$(awk -v base="$baseline_p95" -v best="$best_p95" 'BEGIN { if (base > 0) printf "%.6f", (best - base) / base * 100.0 }')"
  replace_literal p95_delta_percent "$p95_delta"
fi

replace_literal selection_metric "median_s then p95_s"

rm -f "$normalized"

echo "updated: $record"
echo "note: strict correctness, hidden-error, token-sweep, interpretation, and risk fields still require explicit evidence."
