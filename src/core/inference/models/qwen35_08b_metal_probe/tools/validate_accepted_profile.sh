#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/validate_accepted_profile.sh [profile.env]

Validates the accepted Qwen3.5 Metal profile env file.

Rules:
  - every active line must be `export CTOX_QWEN35_<NAME>=<VALUE>`
  - variable names must be unique
  - the file must parse as shell
USAGE
}

if [[ $# -gt 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
profile="${1:-$repo_root/docs/kernel-dev/accepted_profile.env}"

if [[ ! -f "$profile" ]]; then
  echo "missing accepted profile: $profile" >&2
  exit 2
fi

failures=()
active_count=0
names_file="$(mktemp /tmp/ctox_accepted_profile_names.XXXXXX)"

if ! bash -n "$profile"; then
  failures+=("accepted profile does not parse as shell: $profile")
fi

while IFS= read -r line || [[ -n "$line" ]]; do
  stripped="${line#"${line%%[![:space:]]*}"}"
  stripped="${stripped%"${stripped##*[![:space:]]}"}"
  [[ -z "$stripped" ]] && continue
  [[ "$stripped" == \#* ]] && continue

  if [[ ! "$stripped" =~ ^export[[:space:]]+(CTOX_QWEN35_[A-Z0-9_]+)=.+$ ]]; then
    failures+=("invalid active line: $line")
    continue
  fi

  name="${BASH_REMATCH[1]}"
  active_count=$((active_count + 1))
  echo "$name" >> "$names_file"
done < "$profile"

if [[ -s "$names_file" ]]; then
  while IFS= read -r duplicate; do
    [[ -z "$duplicate" ]] && continue
    failures+=("duplicate env var: $duplicate")
  done < <(sort "$names_file" | uniq -d)
fi
rm -f "$names_file"

if [[ "$active_count" -eq 0 ]]; then
  failures+=("accepted profile has no active env flags")
fi

if [[ "${#failures[@]}" -gt 0 ]]; then
  echo "validation: FAIL"
  for failure in "${failures[@]}"; do
    echo "  - $failure"
  done
  exit 1
fi

echo "validation: PASS"
echo "profile: $profile"
echo "active_flags: $active_count"
echo "sha256: $(shasum -a 256 "$profile" | awk '{print $1}')"
