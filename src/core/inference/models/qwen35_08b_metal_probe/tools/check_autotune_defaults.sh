#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/check_autotune_defaults.sh [accepted-profile.env]

Checks that autotune_metalpack_prefill_delta_stack --print-baseline-env matches
the accepted profile for all flags managed by the DeltaNet+FFN autotuner.
Build target/release/autotune_metalpack_prefill_delta_stack first.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
profile="${1:-$repo_root/docs/kernel-dev/accepted_profile.env}"
autotune="$repo_root/target/release/autotune_metalpack_prefill_delta_stack"

if [[ ! -f "$profile" ]]; then
  echo "missing accepted profile: $profile" >&2
  exit 2
fi
if [[ ! -x "$autotune" ]]; then
  echo "missing autotune binary: $autotune" >&2
  echo "run: cargo build --release --bin autotune_metalpack_prefill_delta_stack" >&2
  exit 2
fi

tmp_profile="$(mktemp)"
tmp_autotune="$(mktemp)"
trap 'rm -f "$tmp_profile" "$tmp_autotune"' EXIT

awk '
  /^export CTOX_QWEN35_/ {
    sub(/^export /, "")
    print
  }
' "$profile" | sort > "$tmp_profile"

"$autotune" --print-baseline-env | sort > "$tmp_autotune"

missing=0
while IFS= read -r line; do
  if ! grep -Fx "$line" "$tmp_profile" >/dev/null; then
    echo "accepted profile missing autotune default: $line" >&2
    missing=1
  fi
done < "$tmp_autotune"

stale=0
for family_prefix in \
  CTOX_QWEN35_DELTA_PROJECT_QKVZ_ \
  CTOX_QWEN35_DELTA_OUT_ \
  CTOX_QWEN35_FFN_GATE_UP_ \
  CTOX_QWEN35_DOWN_ \
  CTOX_QWEN35_DELTA_SCAN_ \
  CTOX_QWEN35_DELTA_CONV_SPLIT_
do
  while IFS= read -r line; do
    key="${line%%=*}"
    [[ "$key" == "$family_prefix"* ]] || continue
    if grep -F "$family_prefix" "$tmp_autotune" | grep -Fx "$line" >/dev/null; then
      continue
    fi
    if grep -F "$family_prefix" "$tmp_autotune" >/dev/null; then
      echo "accepted profile has stale/conflicting autotune-family flag: $line" >&2
      stale=1
    fi
  done < "$tmp_profile"
done

if [[ "$missing" -ne 0 || "$stale" -ne 0 ]]; then
  echo "validation: FAIL"
  exit 1
fi

echo "validation: PASS"
echo "profile: $profile"
echo "autotune_baseline_flags: $(wc -l < "$tmp_autotune" | tr -d ' ')"
