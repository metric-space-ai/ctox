#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

build_dir="${TMPDIR:-/tmp}/ctox_qwen35_sme2_smoke"
mkdir -p "$build_dir"
binary="$build_dir/sme2_smoke_probe"

clang -O3 -std=c11 -Wall -Wextra -mcpu=native \
  tools/sme2_smoke_probe.c \
  -o "$binary"

{
  echo "sme2_smoke_probe_runner"
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  sysctl machdep.cpu.brand_string \
    hw.optional.arm.FEAT_SME \
    hw.optional.arm.FEAT_SME2 \
    hw.optional.arm.FEAT_SME2p1 \
    hw.optional.arm.SME_F16F32 \
    hw.optional.arm.SME_I8I32 \
    hw.optional.arm.FEAT_I8MM \
    hw.optional.arm.FEAT_BF16 2>/dev/null || true
  "$binary"
  echo "sme2_disassembly_evidence:"
  otool -tvV "$binary" 2>/dev/null \
    | grep -E 'smstart|smstop|zero[[:space:]]+\{za\}|rdsvl|addsvl' \
    | sed 's/^/  /' || true
}
