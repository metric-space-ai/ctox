#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/list_kernel_experiments.sh [--markdown]

Lists generated kernel experiment records and their validation status.

Default output is a compact table. --markdown writes a Markdown table suitable
for docs/kernel-dev/experiments/INDEX.md.
USAGE
}

markdown=0
if [[ "${1:-}" == "--markdown" ]]; then
  markdown=1
  shift
fi

if [[ $# -ne 0 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

experiment_dir="docs/kernel-dev/experiments"
validator="tools/validate_kernel_experiment.sh"

field_value() {
  local record="$1"
  local key="$2"
  grep -m1 -E "^${key}:" "$record" \
    | sed -E "s/^${key}:[[:space:]]*//" \
    | sed -E 's/[[:space:]]+$//' || true
}

validation_status() {
  local mode="$1"
  local record="$2"
  if [[ "$mode" == "strict" ]]; then
    if "$validator" --strict "$record" >/dev/null 2>&1; then
      echo pass
    else
      echo fail
    fi
  else
    if "$validator" "$record" >/dev/null 2>&1; then
      echo pass
    else
      echo fail
    fi
  fi
}

records=()
record_count=0
if [[ -d "$experiment_dir" ]]; then
  while IFS= read -r record; do
    [[ "$(basename "$record")" == "README.md" ]] && continue
    [[ "$(basename "$record")" == "INDEX.md" ]] && continue
    records+=("$record")
    record_count=$((record_count + 1))
  done < <(find "$experiment_dir" -maxdepth 1 -type f -name '*.md' | sort)
fi

if [[ "$markdown" -eq 1 ]]; then
  echo "| record | date | env_flag | default | strict | decision |"
  echo "| --- | --- | --- | --- | --- | --- |"
else
  printf "%-48s %-16s %-28s %-8s %-8s %-12s\n" \
    "record" "date" "env_flag" "default" "strict" "decision"
fi

if [[ "$record_count" -gt 0 ]]; then
  for record in "${records[@]}"; do
    date_value="$(field_value "$record" date)"
    env_flag="$(field_value "$record" env_flag)"
    decision="$(field_value "$record" decision)"
    default_status="$(validation_status default "$record")"
    strict_status="$(validation_status strict "$record")"
    if [[ -z "$decision" ]]; then
      decision="n/a"
    fi
    if [[ "$markdown" -eq 1 ]]; then
      printf "| %s | %s | %s | %s | %s | %s |\n" \
        "$record" "${date_value:-n/a}" "${env_flag:-n/a}" \
        "$default_status" "$strict_status" "$decision"
    else
      printf "%-48s %-16s %-28s %-8s %-8s %-12s\n" \
        "$(basename "$record")" "${date_value:-n/a}" "${env_flag:-n/a}" \
        "$default_status" "$strict_status" "$decision"
    fi
  done
fi

if [[ "$record_count" -eq 0 && "$markdown" -eq 0 ]]; then
  echo "(no generated experiment records)"
fi
