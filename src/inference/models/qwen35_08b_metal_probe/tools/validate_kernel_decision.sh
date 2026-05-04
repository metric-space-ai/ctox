#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/validate_kernel_decision.sh [--strict] <decision.md>

Checks a kernel decision record for required fields.

Default mode:
  verifies the decision record has basic metadata and a valid decision value.

--strict:
  additionally rejects template placeholders and requires evidence fields needed
  before a decision can justify changing defaults or closing an experiment.
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
  echo "missing decision record: $record" >&2
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

base_required=(
  date
  experiment
  decision
  model
  metalpack
)

for key in "${base_required[@]}"; do
  require_field "$key"
done

decision="$(field_value decision)"
case "$decision" in
  accepted|rejected|opt-in|needs-more-data|"") ;;
  *)
    failures+=("invalid decision value: $decision")
    ;;
esac

experiment="$(field_value experiment)"
if [[ -n "$experiment" && "$experiment" != "n/a" && ! -f "$experiment" ]]; then
  warnings+=("referenced experiment record does not exist locally: $experiment")
fi

if [[ "$strict" -eq 1 ]]; then
  strict_required=(
    one_sentence
    tokens/context
    iterations
    warmup
    baseline_command
    candidate_command
    forensics_record
    search_based
    correctness_gate
    token_sweep_gate
    reference_comparison
    next_experiment
  )
  for key in "${strict_required[@]}"; do
    require_field "$key"
  done

  if [[ "$decision" == "accepted" ]]; then
    require_field accepted_env
    require_field hidden_mean_abs_error
    require_field hidden_rms_error
    require_field hidden_max_abs_error
  fi

  if [[ "$decision" == "rejected" ]]; then
    require_field rejected_env
  fi

  forensics_record="$(field_value forensics_record)"
  if [[ -n "$forensics_record" && "$forensics_record" != "n/a" ]]; then
    if [[ ! -f "$forensics_record" ]]; then
      failures+=("referenced forensics record does not exist locally: $forensics_record")
    elif ! tools/validate_cache_forensics.sh --strict "$forensics_record" >/tmp/ctox_kernel_decision_forensics.$$ 2>&1; then
      failures+=("referenced forensics record does not pass strict validation: $forensics_record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_kernel_decision_forensics.$$
    fi
    rm -f /tmp/ctox_kernel_decision_forensics.$$
  fi

  search_based="$(field_value search_based)"
  case "$search_based" in
    yes|no|n/a|"") ;;
    *)
      failures+=("invalid search_based value: $search_based")
      ;;
  esac

  autotune_record="$(field_value autotune_record)"
  if [[ "$search_based" == "yes" ]]; then
    require_field autotune_record
  fi
  if [[ -n "$autotune_record" && "$autotune_record" != "n/a" ]]; then
    if [[ ! -f "$autotune_record" ]]; then
      failures+=("referenced autotune record does not exist locally: $autotune_record")
    elif ! tools/validate_autotune_record.sh --strict "$autotune_record" >/tmp/ctox_kernel_decision_autotune.$$ 2>&1; then
      failures+=("referenced autotune record does not pass strict validation: $autotune_record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_kernel_decision_autotune.$$
    fi
    rm -f /tmp/ctox_kernel_decision_autotune.$$
  fi

  if grep -nE '<[^>]+>' "$record" >/tmp/ctox_kernel_decision_placeholders.$$; then
    while IFS= read -r line; do
      failures+=("template placeholder still present: $line")
    done </tmp/ctox_kernel_decision_placeholders.$$
  fi
  rm -f /tmp/ctox_kernel_decision_placeholders.$$

  if grep -nE '^[[:space:]]*-[[:space:]]*$' "$record" >/tmp/ctox_kernel_decision_empty_bullets.$$; then
    while IFS= read -r line; do
      warnings+=("empty checklist bullet remains: $line")
    done </tmp/ctox_kernel_decision_empty_bullets.$$
  fi
  rm -f /tmp/ctox_kernel_decision_empty_bullets.$$
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
