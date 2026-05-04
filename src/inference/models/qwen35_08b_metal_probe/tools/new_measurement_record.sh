#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/new_measurement_record.sh <experiment.md> <capture-dir> <kind> [slug]

Creates a measurement record from a directory produced by
tools/capture_measurement_output.sh.

kind must be one of:
  baseline
  candidate
  forensics
  autotune
  correctness
  smoke
USAGE
}

if [[ $# -lt 3 || $# -gt 4 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

experiment="$1"
capture_dir="$2"
kind="$3"
slug="${4:-$(basename "$experiment" .md)-$kind}"

case "$kind" in
  baseline|candidate|forensics|autotune|correctness|smoke) ;;
  *)
    echo "invalid measurement kind: $kind" >&2
    usage
    exit 2
    ;;
esac

if [[ ! -f "$experiment" ]]; then
  echo "missing experiment record: $experiment" >&2
  exit 2
fi
if [[ ! -d "$capture_dir" ]]; then
  echo "missing capture dir: $capture_dir" >&2
  exit 2
fi
if [[ ! "$slug" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo "invalid slug '$slug'; use only letters, numbers, dot, underscore, or dash" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

template="docs/kernel-dev/MEASUREMENT_RECORD_TEMPLATE.md"
out_dir="docs/kernel-dev/measurements"
mkdir -p "$out_dir"

timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
out="$out_dir/${timestamp}-${slug}.md"

field_value() {
  local file="$1"
  local key="$2"
  [[ -f "$file" ]] || return 0
  awk -v key="$key" '
    index($0, key ":") == 1 {
      sub(/^[^:]*:[ \t]*/, "")
      sub(/[ \t]+$/, "")
      print
      exit
    }
  ' "$file"
}

replace_field() {
  local key="$1"
  local value="$2"
  KEY="$key" VALUE="$value" perl -0pi -e '
    my $key = $ENV{"KEY"};
    my $value = $ENV{"VALUE"};
    s/^(\Q$key\E:).*$/$1 $value/m;
  ' "$out"
}

manifest="$capture_dir/manifest.txt"
stdout="$capture_dir/stdout.txt"
stderr="$capture_dir/stderr.txt"
normalized="$capture_dir/normalized.txt"
exit_code_file="$capture_dir/exit_code.txt"
exit_code="n/a"
if [[ -f "$exit_code_file" ]]; then
  exit_code="$(tr -d '[:space:]' < "$exit_code_file")"
fi

{
  cat <<HEADER
# Measurement: $slug

Generated: $timestamp

HEADER
  cat "$template"
} > "$out"

replace_field "date" "$timestamp"
replace_field "experiment" "$experiment"
replace_field "label" "$(field_value "$manifest" label)"
replace_field "capture_dir" "$capture_dir"
replace_field "manifest" "$manifest"
replace_field "stdout" "$stdout"
replace_field "stderr" "$stderr"
replace_field "normalized" "$normalized"
replace_field "exit_code_file" "$exit_code_file"
replace_field "exit_code" "$exit_code"
replace_field "command" "$(field_value "$manifest" command)"
replace_field "accepted_profile_hash" "$(field_value "$manifest" accepted_profile_hash)"
replace_field "git_commit" "$(field_value "$manifest" git_commit)"
replace_field "git_dirty_state" "$(field_value "$manifest" git_dirty_state)"
replace_field "tokens/context" "$(field_value "$normalized" tokens/context)"
replace_field "iterations" "$(field_value "$normalized" iterations)"
replace_field "warmup" "$(field_value "$normalized" warmup)"
replace_field "median_s" "$(field_value "$normalized" median_s)"
replace_field "p95_s" "$(field_value "$normalized" p95_s)"
replace_field "effective_GB/s" "$(field_value "$normalized" effective_GB/s)"
replace_field "checksum" "$(field_value "$normalized" checksum)"
replace_field "output_csv" "$(field_value "$normalized" output_csv)"
replace_field "measurement_kind" "$kind"
replace_field "usable_for_decision" "no"
replace_field "notes" "raw capture imported; strict decision usability requires explicit review"

if [[ -x tools/update_measurement_index.sh ]]; then
  tools/update_measurement_index.sh >/dev/null
fi

echo "measurement_record: $out"
echo "index:              $repo_root/docs/kernel-dev/measurements/INDEX.md"
