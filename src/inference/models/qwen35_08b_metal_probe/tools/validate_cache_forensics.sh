#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/validate_cache_forensics.sh [--strict] <forensics.md>

Checks a cache/memory forensics record for required metadata and byte-model
fields.

Default mode:
  verifies basic metadata and a valid evidence level.

--strict:
  additionally rejects template placeholders and requires runtime, byte-model,
  and interpretation fields needed before a forensics record can support a
  kernel decision.
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
  echo "missing forensics record: $record" >&2
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
  op
  model
  metalpack
  evidence_level
)

for key in "${base_required[@]}"; do
  require_field "$key"
done

evidence_level="$(field_value evidence_level)"
case "$evidence_level" in
  inferred-only|hardware-counter-backed|"") ;;
  *)
    failures+=("invalid evidence_level: $evidence_level")
    ;;
esac

experiment="$(field_value experiment)"
if [[ -n "$experiment" && "$experiment" != "n/a" && ! -f "$experiment" ]]; then
  warnings+=("referenced experiment record does not exist locally: $experiment")
fi

if [[ "$evidence_level" == "hardware-counter-backed" ]]; then
  require_field counter_source
fi

if [[ "$strict" -eq 1 ]]; then
  strict_required=(
    kernel
    selection_env
    tokens/context
    command
    compulsory_miss_floor
    avoidable_miss_suspect
    occupancy_suspect
    scratch_suspect
    cpu_overhead_suspect
    decision
    next_action
  )
  for key in "${strict_required[@]}"; do
    require_field "$key"
  done

  numeric_required=(
    median_s
    p95_s
    effective_GB/s
    unique_weight_bytes
    weight_group_stream_bytes
    logical_operand_weight_bytes
    modeled_dram_miss_bytes
    modeled_cache_hit_bytes
    non_weight_bytes
    scratch_write_bytes
    scratch_read_bytes
    persistent_state_bytes
    tail_underfill
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

  if grep -nE '<[^>]+>' "$record" >/tmp/ctox_cache_forensics_placeholders.$$; then
    while IFS= read -r line; do
      failures+=("template placeholder still present: $line")
    done </tmp/ctox_cache_forensics_placeholders.$$
  fi
  rm -f /tmp/ctox_cache_forensics_placeholders.$$
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
