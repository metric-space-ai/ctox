#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/check_kernel_promotion.sh <decision.md>

Checks whether a kernel decision record is strong enough to promote a candidate
into the accepted profile.

This script does not run benchmarks. It validates that:
  - the decision record passes strict validation
  - the decision is `accepted`
  - the referenced experiment exists and passes strict validation
  - linked forensics/autotune records pass strict validation through the decision
USAGE
}

if [[ $# -ne 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

record="$1"
if [[ ! -f "$record" ]]; then
  echo "missing decision record: $record" >&2
  exit 2
fi

failures=()

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

capture_failure() {
  local label="$1"
  local file="$2"
  local tmp="$3"
  failures+=("$label failed: $file")
  while IFS= read -r line; do
    failures+=("  $line")
  done < "$tmp"
}

decision="$(field_value "$record" decision)"
experiment="$(field_value "$record" experiment)"

if ! tools/validate_kernel_decision.sh --strict "$record" >/tmp/ctox_kernel_promotion_decision.$$ 2>&1; then
  capture_failure "strict decision validation" "$record" /tmp/ctox_kernel_promotion_decision.$$
fi
rm -f /tmp/ctox_kernel_promotion_decision.$$

if [[ "$decision" != "accepted" ]]; then
  failures+=("decision is not accepted: ${decision:-missing}")
fi

if [[ -z "$experiment" || "$experiment" == "n/a" ]]; then
  failures+=("decision does not reference an experiment record")
elif [[ ! -f "$experiment" ]]; then
  failures+=("referenced experiment record does not exist locally: $experiment")
else
  if ! tools/validate_kernel_experiment.sh --strict "$experiment" >/tmp/ctox_kernel_promotion_experiment.$$ 2>&1; then
    capture_failure "strict experiment validation" "$experiment" /tmp/ctox_kernel_promotion_experiment.$$
  fi
  rm -f /tmp/ctox_kernel_promotion_experiment.$$
fi

if [[ "${#failures[@]}" -gt 0 ]]; then
  echo "promotion: BLOCKED"
  for failure in "${failures[@]}"; do
    echo "  - $failure"
  done
  exit 1
fi

echo "promotion: PASS"
echo "decision: $record"
echo "experiment: $experiment"
forensics_record="$(field_value "$record" forensics_record)"
autotune_record="$(field_value "$record" autotune_record)"
if [[ -n "$forensics_record" && "$forensics_record" != "n/a" ]]; then
  echo "forensics: $forensics_record"
fi
if [[ -n "$autotune_record" && "$autotune_record" != "n/a" ]]; then
  echo "autotune: $autotune_record"
fi
