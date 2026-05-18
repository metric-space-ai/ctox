#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/validate_measurement_record.sh [--strict] <measurement.md>

Checks a measurement record and its referenced capture files.

Default mode:
  verifies metadata, referenced capture files, and a valid measurement kind.

--strict:
  additionally requires a successful exit code, command/profile/git metadata,
  and normalized runtime fields.
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
  echo "missing measurement record: $record" >&2
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
  capture_dir
  manifest
  stdout
  stderr
  normalized
  exit_code_file
  exit_code
  measurement_kind
)

for key in "${base_required[@]}"; do
  require_field "$key"
done

kind="$(field_value measurement_kind)"
case "$kind" in
  baseline|candidate|forensics|autotune|correctness|smoke|"") ;;
  *)
    failures+=("invalid measurement_kind: $kind")
    ;;
esac

for key in experiment manifest stdout stderr normalized exit_code_file; do
  path="$(field_value "$key")"
  if [[ -n "$path" && "$path" != "n/a" && ! -f "$path" ]]; then
    failures+=("referenced file does not exist: $key=$path")
  fi
done

capture_dir="$(field_value capture_dir)"
if [[ -n "$capture_dir" && "$capture_dir" != "n/a" && ! -d "$capture_dir" ]]; then
  failures+=("capture_dir does not exist: $capture_dir")
fi

if [[ "$strict" -eq 1 ]]; then
  strict_required=(
    command
    accepted_profile_hash
    git_commit
    git_dirty_state
    usable_for_decision
  )
  for key in "${strict_required[@]}"; do
    require_field "$key"
  done

  require_number_field tokens/context
  require_number_field iterations
  require_number_field median_s
  require_number_field p95_s

  exit_code="$(field_value exit_code)"
  if [[ "$exit_code" != "0" ]]; then
    failures+=("measurement exit_code is not zero: $exit_code")
  fi

  usable="$(field_value usable_for_decision)"
  case "$usable" in
    yes|no|"") ;;
    *)
      failures+=("invalid usable_for_decision value: $usable")
      ;;
  esac

  if grep -nE '<[^>]+>' "$record" >/tmp/ctox_measurement_placeholders.$$; then
    while IFS= read -r line; do
      failures+=("template placeholder still present: $line")
    done </tmp/ctox_measurement_placeholders.$$
  fi
  rm -f /tmp/ctox_measurement_placeholders.$$
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
