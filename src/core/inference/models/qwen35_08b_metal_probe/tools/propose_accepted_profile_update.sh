#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/propose_accepted_profile_update.sh <decision.md> [slug]

Creates an accepted-profile update proposal only after the promotion gate passes.
This script does not edit docs/kernel-dev/accepted_profile.env.
USAGE
}

if [[ $# -lt 1 || $# -gt 2 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

decision_record="$1"
slug="${2:-$(basename "$decision_record" .md)}"

if [[ ! "$slug" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo "invalid slug '$slug'; use only letters, numbers, dot, underscore, or dash" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ ! -f "$decision_record" ]]; then
  echo "missing decision record: $decision_record" >&2
  exit 2
fi

promotion_log="$(mktemp /tmp/ctox_profile_promotion.XXXXXX.txt)"
if ! tools/check_kernel_promotion.sh "$decision_record" >"$promotion_log" 2>&1; then
  echo "profile update: BLOCKED"
  while IFS= read -r line; do
    echo "  $line"
  done < "$promotion_log"
  rm -f "$promotion_log"
  exit 1
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

replace_field() {
  local key="$1"
  local value="$2"
  KEY="$key" VALUE="$value" perl -0pi -e '
    my $key = $ENV{"KEY"};
    my $value = $ENV{"VALUE"};
    s/^(\Q$key\E:).*$/$1 $value/m;
  ' "$out"
}

template="docs/kernel-dev/ACCEPTED_PROFILE_UPDATE_TEMPLATE.md"
profile="docs/kernel-dev/accepted_profile.env"
out_dir="docs/kernel-dev/profile-updates"
mkdir -p "$out_dir"

timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
out="$out_dir/${timestamp}-${slug}.md"
profile_hash="n/a"
if [[ -f "$profile" ]]; then
  profile_hash="$(shasum -a 256 "$profile" | awk '{print $1}')"
fi

{
  cat <<HEADER
# Accepted Profile Update: $slug

Generated: $timestamp

HEADER
  cat "$template"
} > "$out"

experiment_record="$(field_value "$decision_record" experiment)"
forensics_record="$(field_value "$decision_record" forensics_record)"
autotune_record="$(field_value "$decision_record" autotune_record)"

replace_field "date" "$timestamp"
replace_field "decision_record" "$decision_record"
replace_field "experiment_record" "$experiment_record"
replace_field "forensics_record" "${forensics_record:-n/a}"
replace_field "autotune_record" "${autotune_record:-n/a}"
replace_field "accepted_profile_path" "$profile"
replace_field "accepted_profile_hash_before" "$profile_hash"
replace_field "accepted_env" "$(field_value "$decision_record" accepted_env)"
replace_field "one_sentence" "$(field_value "$decision_record" one_sentence)"
replace_field "promotion_check" "passed"
replace_field "correctness_gate" "$(field_value "$decision_record" correctness_gate)"
replace_field "token_sweep_gate" "$(field_value "$decision_record" token_sweep_gate)"
replace_field "reference_comparison" "$(field_value "$decision_record" reference_comparison)"
replace_field "cache_forensics" "${forensics_record:-n/a}"
replace_field "autotune_evidence" "${autotune_record:-n/a}"
replace_field "manual_apply_required" "yes"
replace_field "profile_lines_to_add_or_change" "$(field_value "$decision_record" accepted_env)"
replace_field "rollback_plan" "restore $profile to hash $profile_hash"

rm -f "$promotion_log"

if [[ -x tools/update_accepted_profile_update_index.sh ]]; then
  tools/update_accepted_profile_update_index.sh >/dev/null
fi

echo "profile_update: $out"
echo "index:          $repo_root/docs/kernel-dev/profile-updates/INDEX.md"
