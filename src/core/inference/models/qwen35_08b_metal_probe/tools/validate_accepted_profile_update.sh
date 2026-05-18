#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/validate_accepted_profile_update.sh [--strict] <profile-update.md>

Checks an accepted-profile update proposal.

Default mode verifies required metadata. Strict mode also re-runs the promotion
gate for the linked decision and checks the recorded profile hash.
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
  echo "missing profile update record: $record" >&2
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
  decision_record
  experiment_record
  accepted_profile_path
  accepted_profile_hash_before
  accepted_env
)

for key in "${base_required[@]}"; do
  require_field "$key"
done

decision_record="$(field_value decision_record)"
if [[ -n "$decision_record" && "$decision_record" != "n/a" && ! -f "$decision_record" ]]; then
  failures+=("decision record does not exist locally: $decision_record")
fi

if [[ "$strict" -eq 1 ]]; then
  strict_required=(
    one_sentence
    promotion_check
    correctness_gate
    token_sweep_gate
    reference_comparison
    cache_forensics
    manual_apply_required
    profile_lines_to_add_or_change
    rollback_plan
  )
  for key in "${strict_required[@]}"; do
    require_field "$key"
  done

  if [[ -n "$decision_record" && "$decision_record" != "n/a" && -f "$decision_record" ]]; then
    if ! tools/check_kernel_promotion.sh "$decision_record" >/tmp/ctox_profile_update_promotion.$$ 2>&1; then
      failures+=("linked decision no longer passes promotion gate: $decision_record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_profile_update_promotion.$$
    fi
    rm -f /tmp/ctox_profile_update_promotion.$$
  fi

  profile_path="$(field_value accepted_profile_path)"
  recorded_hash="$(field_value accepted_profile_hash_before)"
  if [[ -n "$profile_path" && "$profile_path" != "n/a" && -f "$profile_path" && -n "$recorded_hash" && "$recorded_hash" != "n/a" ]]; then
    actual_hash="$(shasum -a 256 "$profile_path" | awk '{print $1}')"
    if [[ "$actual_hash" != "$recorded_hash" ]]; then
      warnings+=("accepted profile hash changed since proposal: recorded=$recorded_hash actual=$actual_hash")
    fi
  fi

  if grep -nE '<[^>]+>' "$record" >/tmp/ctox_profile_update_placeholders.$$; then
    while IFS= read -r line; do
      failures+=("template placeholder still present: $line")
    done </tmp/ctox_profile_update_placeholders.$$
  fi
  rm -f /tmp/ctox_profile_update_placeholders.$$
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
