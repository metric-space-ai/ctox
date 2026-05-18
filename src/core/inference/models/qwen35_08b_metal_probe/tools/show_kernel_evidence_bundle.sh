#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/show_kernel_evidence_bundle.sh <decision.md>

Prints the validation status of a decision record and its linked experiment,
cache-forensics, and autotune evidence records.

This script does not run benchmarks.
USAGE
}

if [[ $# -ne 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

decision_record="$1"
if [[ ! -f "$decision_record" ]]; then
  echo "missing decision record: $decision_record" >&2
  exit 2
fi

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

validator_status() {
  local mode="$1"
  local validator="$2"
  local file="$3"

  if [[ -z "$file" || "$file" == "n/a" || ! -f "$file" ]]; then
    echo missing
    return
  fi

  if [[ "$mode" == "strict" ]]; then
    if "$validator" --strict "$file" >/dev/null 2>&1; then
      echo pass
    else
      echo fail
    fi
  else
    if "$validator" "$file" >/dev/null 2>&1; then
      echo pass
    else
      echo fail
    fi
  fi
}

promotion_status() {
  local file="$1"
  if tools/check_kernel_promotion.sh "$file" >/dev/null 2>&1; then
    echo pass
  else
    echo blocked
  fi
}

experiment_record="$(field_value "$decision_record" experiment)"
forensics_record="$(field_value "$decision_record" forensics_record)"
autotune_record="$(field_value "$decision_record" autotune_record)"

echo "kernel evidence bundle"
echo "decision_record: $decision_record"
echo "decision: $(field_value "$decision_record" decision)"
echo "search_based: $(field_value "$decision_record" search_based)"
echo "promotion: $(promotion_status "$decision_record")"
echo
printf "%-12s %-8s %-8s %s\n" "artifact" "default" "strict" "path"
printf "%-12s %-8s %-8s %s\n" \
  "decision" \
  "$(validator_status default tools/validate_kernel_decision.sh "$decision_record")" \
  "$(validator_status strict tools/validate_kernel_decision.sh "$decision_record")" \
  "$decision_record"
printf "%-12s %-8s %-8s %s\n" \
  "experiment" \
  "$(validator_status default tools/validate_kernel_experiment.sh "$experiment_record")" \
  "$(validator_status strict tools/validate_kernel_experiment.sh "$experiment_record")" \
  "${experiment_record:-n/a}"
printf "%-12s %-8s %-8s %s\n" \
  "forensics" \
  "$(validator_status default tools/validate_cache_forensics.sh "$forensics_record")" \
  "$(validator_status strict tools/validate_cache_forensics.sh "$forensics_record")" \
  "${forensics_record:-n/a}"
printf "%-12s %-8s %-8s %s\n" \
  "autotune" \
  "$(validator_status default tools/validate_autotune_record.sh "$autotune_record")" \
  "$(validator_status strict tools/validate_autotune_record.sh "$autotune_record")" \
  "${autotune_record:-n/a}"
