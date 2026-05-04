#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/new_candidate_manifest.sh <candidate-id> <source> <kernel-or-file> <bottleneck> [slug]

Creates a timestamped candidate manifest under docs/kernel-dev/candidates/.
This script does not run benchmarks.
USAGE
}

if [[ $# -lt 4 || $# -gt 5 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

candidate_id="$1"
source="$2"
kernel_or_file="$3"
bottleneck="$4"
slug="${5:-$candidate_id}"

if [[ ! "$slug" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo "invalid slug '$slug'; use only letters, numbers, dot, underscore, or dash" >&2
  exit 2
fi

case "$source" in
  manual|autotune|openevolve|paper|llama.cpp|luce|other) ;;
  *)
    echo "invalid source '$source'" >&2
    exit 2
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
template="$repo_root/docs/kernel-dev/CANDIDATE_MANIFEST_TEMPLATE.md"
out_dir="$repo_root/docs/kernel-dev/candidates"
mkdir -p "$out_dir"

timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
out="$out_dir/${timestamp}-${slug}.md"

{
  cat <<HEADER
# Kernel Candidate Manifest - $candidate_id

Generated: $timestamp

HEADER
  cat "$template"
} > "$out"

replace_field() {
  local key="$1"
  local value="$2"
  KEY="$key" VALUE="$value" perl -0pi -e '
    my $key = $ENV{"KEY"};
    my $value = $ENV{"VALUE"};
    s/^(\Q$key\E:).*$/$1 $value/m;
  ' "$out"
}

replace_field "date" "$timestamp"
replace_field "candidate_id" "$candidate_id"
replace_field "source" "$source"
replace_field "owner" "main-thread"
replace_field "status" "proposed"
replace_field "kernel_or_file" "$kernel_or_file"
replace_field "intended_bottleneck" "$bottleneck"

echo "candidate_manifest: $out"
