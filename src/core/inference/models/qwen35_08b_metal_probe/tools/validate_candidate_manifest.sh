#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/validate_candidate_manifest.sh [--strict] <candidate.md>

Checks a kernel candidate manifest. This is intended for hand-written,
autotuned, and OpenEvolve-style generated kernel/layout candidates before they
are promoted into paired benchmark records.

Default mode verifies the core metadata exists.
--strict also rejects placeholders and requires correctness, benchmark, result,
and decision fields.
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
  echo "missing candidate manifest: $record" >&2
  exit 2
fi

failures=()

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
  candidate_id
  source
  owner
  status
  kernel_or_file
  intended_bottleneck
)

for key in "${base_required[@]}"; do
  require_field "$key"
done

source="$(field_value source)"
case "$source" in
  manual|autotune|openevolve|paper|llama.cpp|luce|other|"") ;;
  *) failures+=("invalid source: $source") ;;
esac

status="$(field_value status)"
case "$status" in
  proposed|implemented|measured|accepted|rejected|abandoned|"") ;;
  *) failures+=("invalid status: $status") ;;
esac

if [[ "$strict" -eq 1 ]]; then
  strict_required=(
    expected_win
    cache_or_scratch_hypothesis
    risk
    parameters
    fixed_constraints
    gate_type
    approximate_mode
    tokens_or_contexts
    paired_order
    reference
    median_delta_percent
    p95_delta_percent
    roofline_class
    metal_error_stats
    decision
    accept_reject_reason
    next_action
  )
  for key in "${strict_required[@]}"; do
    require_field "$key"
  done

  decision="$(field_value decision)"
  case "$decision" in
    accept|reject|keep-opt-in|needs-more-data|"") ;;
    *) failures+=("invalid decision: $decision") ;;
  esac

  approximate_mode="$(field_value approximate_mode)"
  case "$approximate_mode" in
    yes|no|"") ;;
    *) failures+=("invalid approximate_mode: $approximate_mode") ;;
  esac

  if grep -nE '<[^>]+>' "$record" >/tmp/ctox_candidate_placeholders.$$; then
    while IFS= read -r line; do
      failures+=("template placeholder still present: $line")
    done </tmp/ctox_candidate_placeholders.$$
  fi
  rm -f /tmp/ctox_candidate_placeholders.$$
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
