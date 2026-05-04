#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/run_accepted_profile.sh <command> [args...]

Sources docs/kernel-dev/accepted_profile.env and runs the command with the
conservative accepted Qwen3.5-0.8B Metal env flags.

Examples:
  tools/run_accepted_profile.sh printenv CTOX_QWEN35_DELTA_SCAN_ROWCACHE
  tools/run_accepted_profile.sh target/release/memory_forensics \
    /tmp/ctox_qwen35_08b_real_fp16.metalpack 4096 3 150
USAGE
}

if [[ $# -lt 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
profile="$repo_root/docs/kernel-dev/accepted_profile.env"
if [[ ! -f "$profile" ]]; then
  echo "missing accepted profile: $profile" >&2
  exit 1
fi

if [[ -x "$repo_root/tools/validate_accepted_profile.sh" ]]; then
  "$repo_root/tools/validate_accepted_profile.sh" "$profile" >/dev/null
fi

# shellcheck source=/dev/null
source "$profile"

exec "$@"
