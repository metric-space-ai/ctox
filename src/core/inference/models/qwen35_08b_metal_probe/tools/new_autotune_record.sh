#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/new_autotune_record.sh <experiment.md> <parameter-family> [slug]

Creates a timestamped autotune record under docs/kernel-dev/autotune/ and copies
basic metadata from the experiment record.

This script does not run benchmarks.
USAGE
}

if [[ $# -lt 2 || $# -gt 3 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

experiment="$1"
family="$2"
slug="${3:-$(basename "$experiment" .md)-$family}"

if [[ ! -f "$experiment" ]]; then
  echo "missing experiment record: $experiment" >&2
  exit 2
fi

if [[ ! "$slug" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo "invalid slug '$slug'; use only letters, numbers, dot, underscore, or dash" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
template="$repo_root/docs/kernel-dev/AUTOTUNE_RECORD_TEMPLATE.md"
out_dir="$repo_root/docs/kernel-dev/autotune"
mkdir -p "$out_dir"

timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
out="$out_dir/${timestamp}-${slug}.md"

field_value() {
  local key="$1"
  awk -v key="$key" '
    index($0, key ":") == 1 {
      sub(/^[^:]*:[ \t]*/, "")
      sub(/[ \t]+$/, "")
      print
      exit
    }
  ' "$experiment"
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

{
  cat <<HEADER
# Autotune: $slug

Generated: $timestamp

HEADER
  cat "$template"
} > "$out"

replace_field "date" "$timestamp"
replace_field "experiment" "$experiment"
replace_field "parameter_family" "$family"
replace_field "model" "$(field_value model)"
replace_field "metalpack" "$(field_value metalpack_path)"
replace_field "binary_path" "$(field_value binary_path)"
replace_field "output_csv" "$(field_value output_csv)"

if [[ -x "$repo_root/tools/update_autotune_index.sh" ]]; then
  "$repo_root/tools/update_autotune_index.sh" >/dev/null
fi

echo "autotune_record: $out"
echo "index:           $repo_root/docs/kernel-dev/autotune/INDEX.md"
