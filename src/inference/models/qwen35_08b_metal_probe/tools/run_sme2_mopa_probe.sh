#!/usr/bin/env bash
set -euo pipefail

repeats="${1:-100000}"
iterations="${2:-5}"
warmup="${3:-1}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

build_dir="${TMPDIR:-/tmp}/ctox_qwen35_sme2_mopa"
mkdir -p "$build_dir"
binary="$build_dir/sme2_mopa_probe"

clang -O3 -std=c11 -Wall -Wextra -mcpu=native \
  tools/sme2_mopa_probe.c \
  -o "$binary"

{
  echo "sme2_mopa_probe_runner"
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  sysctl machdep.cpu.brand_string \
    hw.optional.arm.FEAT_SME \
    hw.optional.arm.FEAT_SME2 \
    hw.optional.arm.SME_I8I32 \
    hw.optional.arm.FEAT_I8MM 2>/dev/null || true
  "$binary" "$repeats" "$iterations" "$warmup"
  echo "sme2_mopa_disassembly_evidence:"
  otool -tvV "$binary" 2>/dev/null \
    | grep -E 'smopa|umopa|mopa|smstart|smstop|zero[[:space:]]+\{za\}' \
    | sed 's/^/  /' || true
}
