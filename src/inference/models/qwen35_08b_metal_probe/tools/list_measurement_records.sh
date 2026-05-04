#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/list_measurement_records.sh [--markdown]

Lists measurement records and validation status.
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

measurement_dir="docs/kernel-dev/measurements"
validator="tools/validate_measurement_record.sh"

field_value() {
  local record="$1"
  local key="$2"
  awk -v key="$key" '
    index($0, key ":") == 1 {
      sub(/^[^:]*:[ \t]*/, "")
      sub(/[ \t]+$/, "")
      print
      exit
    }
  ' "$record"
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
if [[ -d "$measurement_dir" ]]; then
  while IFS= read -r record; do
    [[ "$(basename "$record")" == "README.md" ]] && continue
    [[ "$(basename "$record")" == "INDEX.md" ]] && continue
    records+=("$record")
    record_count=$((record_count + 1))
  done < <(find "$measurement_dir" -maxdepth 1 -type f -name '*.md' | sort)
fi

if [[ "$markdown" -eq 1 ]]; then
  echo "| record | date | experiment | kind | exit | default | strict |"
  echo "| --- | --- | --- | --- | --- | --- | --- |"
else
  printf "%-48s %-16s %-42s %-12s %-6s %-8s %-8s\n" \
    "record" "date" "experiment" "kind" "exit" "default" "strict"
fi

if [[ "$record_count" -gt 0 ]]; then
  for record in "${records[@]}"; do
    date_value="$(field_value "$record" date)"
    experiment="$(field_value "$record" experiment)"
    kind="$(field_value "$record" measurement_kind)"
    exit_code="$(field_value "$record" exit_code)"
    default_status="$(validation_status default "$record")"
    strict_status="$(validation_status strict "$record")"
    if [[ "$markdown" -eq 1 ]]; then
      printf "| %s | %s | %s | %s | %s | %s | %s |\n" \
        "$record" "${date_value:-n/a}" "${experiment:-n/a}" "${kind:-n/a}" \
        "${exit_code:-n/a}" "$default_status" "$strict_status"
    else
      printf "%-48s %-16s %-42s %-12s %-6s %-8s %-8s\n" \
        "$(basename "$record")" "${date_value:-n/a}" "$(basename "${experiment:-n/a}")" \
        "${kind:-n/a}" "${exit_code:-n/a}" "$default_status" "$strict_status"
    fi
  done
fi

if [[ "$record_count" -eq 0 && "$markdown" -eq 0 ]]; then
  echo "(no measurement records)"
fi
