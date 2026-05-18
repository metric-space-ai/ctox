#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/fill_forensics_record_from_output.sh <benchmark-output.txt> <forensics-record.md>

Fills extractable runtime fields in a cache/memory forensics record from
existing benchmark stdout. It does not run benchmarks and does not fill byte
model or interpretation fields.
USAGE
}

if [[ $# -ne 2 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

output="$1"
record="$2"

if [[ ! -f "$output" ]]; then
  echo "missing benchmark output: $output" >&2
  exit 2
fi
if [[ ! -f "$record" ]]; then
  echo "missing forensics record: $record" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

normalized="$(mktemp /tmp/ctox_forensics_normalized.XXXXXX)"
tools/normalize_benchmark_output.sh "$output" > "$normalized"

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

replace_field_if_present() {
  local source_key="$1"
  local target_key="$2"
  local value
  value="$(field_value "$normalized" "$source_key")"
  if [[ -z "$value" ]]; then
    return
  fi
  KEY="$target_key" VALUE="$value" perl -0pi -e '
    my $key = $ENV{"KEY"};
    my $value = $ENV{"VALUE"};
    s/^(\Q$key\E:).*$/$1 $value/m;
  ' "$record"
}

replace_literal() {
  local target_key="$1"
  local value="$2"
  KEY="$target_key" VALUE="$value" perl -0pi -e '
    my $key = $ENV{"KEY"};
    my $value = $ENV{"VALUE"};
    s/^(\Q$key\E:).*$/$1 $value/m;
  ' "$record"
}

replace_field_if_present tokens/context tokens/context
replace_field_if_present median_s median_s
replace_field_if_present p95_s p95_s
replace_field_if_present effective_GB/s effective_GB/s

tokens="$(field_value "$normalized" tokens/context)"
median_s="$(field_value "$normalized" median_s)"
if [[ -n "$tokens" && -n "$median_s" ]]; then
  tok_s="$(awk -v tokens="$tokens" -v median="$median_s" 'BEGIN { if (median > 0) printf "%.6f", tokens / median }')"
  replace_literal tok_s "$tok_s"
fi

replace_literal command "source_output=$output"
replace_literal evidence_level "inferred-only"

rm -f "$normalized"

echo "updated: $record"
echo "note: strict byte-model and interpretation fields still require explicit cache/memory evidence."
