#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/validate_autotune_record.sh [--strict] <autotune.md>

Checks an autotune evidence record.

Default mode:
  verifies basic metadata exists.

--strict:
  additionally rejects template placeholders and requires search-space,
  best-candidate, correctness, and token-sweep evidence.
USAGE
}

strict=0
if [[ "${1:-}" == "--strict" ]]; then
  strict=1
  shift
fi

if [[ $# -ne 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

record="$1"
if [[ ! -f "$record" ]]; then
  echo "missing autotune record: $record" >&2
  exit 2
fi

failures=()
warnings=()

field_value() {
  local key="$1"
  awk -v key="$key" '
    index($0, key ":") == 1 {
      sub(/^[^:]*:[ \t]*/, "")
      sub(/[ \t]+$/, "")
      print
      exit
    }
  ' "$record"
}

require_field() {
  local key="$1"
  local value
  value="$(field_value "$key")"
  if [[ -z "$value" || "$value" == "n/a" || "$value" == "-" ]]; then
    failures+=("missing required field: $key")
  fi
}

require_number_field() {
  local key="$1"
  local value
  value="$(field_value "$key")"
  if [[ -z "$value" || "$value" == "n/a" || "$value" == "-" ]]; then
    failures+=("missing numeric field: $key")
  elif [[ ! "$value" =~ ^-?[0-9]+([.][0-9]+)?$ ]]; then
    failures+=("numeric field is not a number: $key=$value")
  fi
}

base_required=(
  date
  experiment
  parameter_family
  model
  metalpack
)

for key in "${base_required[@]}"; do
  require_field "$key"
done

experiment="$(field_value experiment)"
if [[ -n "$experiment" && "$experiment" != "n/a" && ! -f "$experiment" ]]; then
  warnings+=("referenced experiment record does not exist locally: $experiment")
fi

if [[ "$strict" -eq 1 ]]; then
  strict_required=(
    binary_path
    output_csv
    tokens/context
    iterations
    warmup
    search_space
    baseline_selection
    best_selection
    chosen_env
    rejected_candidates_path
    selection_metric
    correctness_gate
    token_sweep_gate
    why_best_won
    why_others_lost
    risk
    decision
    next_action
  )
  for key in "${strict_required[@]}"; do
    require_field "$key"
  done

  numeric_required=(
    candidate_count
    baseline_median_s
    baseline_p95_s
    baseline_tok_s
    best_median_s
    best_p95_s
    best_tok_s
    median_delta_percent
    p95_delta_percent
    hidden_mean_abs_error
    hidden_rms_error
    hidden_max_abs_error
    checksum_delta
  )
  for key in "${numeric_required[@]}"; do
    require_number_field "$key"
  done

  decision="$(field_value decision)"
  case "$decision" in
    accepted|rejected|opt-in|needs-more-data|"") ;;
    *)
      failures+=("invalid decision value: $decision")
      ;;
  esac

  cache_forensics_record="$(field_value cache_forensics_record)"
  if [[ -n "$cache_forensics_record" && "$cache_forensics_record" != "n/a" ]]; then
    if [[ ! -f "$cache_forensics_record" ]]; then
      failures+=("referenced cache forensics record does not exist locally: $cache_forensics_record")
    elif ! tools/validate_cache_forensics.sh --strict "$cache_forensics_record" >/tmp/ctox_autotune_forensics.$$ 2>&1; then
      failures+=("referenced cache forensics record does not pass strict validation: $cache_forensics_record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_autotune_forensics.$$
    fi
    rm -f /tmp/ctox_autotune_forensics.$$
  fi

  if grep -nE '<[^>]+>' "$record" >/tmp/ctox_autotune_placeholders.$$; then
    while IFS= read -r line; do
      failures+=("template placeholder still present: $line")
    done </tmp/ctox_autotune_placeholders.$$
  fi
  rm -f /tmp/ctox_autotune_placeholders.$$
fi

if [[ "${#warnings[@]}" -gt 0 ]]; then
  echo "warnings:"
  for warning in "${warnings[@]}"; do
    echo "  - $warning"
  done
fi

if [[ "${#failures[@]}" -gt 0 ]]; then
  echo "validation: FAIL"
  for failure in "${failures[@]}"; do
    echo "  - $failure"
  done
  exit 1
fi

echo "validation: PASS"
if [[ "$strict" -eq 1 ]]; then
  echo "mode: strict"
else
  echo "mode: default"
fi
