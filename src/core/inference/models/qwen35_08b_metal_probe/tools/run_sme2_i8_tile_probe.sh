#!/usr/bin/env bash
set -euo pipefail

tokens="${1:-512}"
rows="${2:-3584}"
k="${3:-1024}"
iterations="${4:-5}"
warmup="${5:-1}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

build_dir="${TMPDIR:-/tmp}/ctox_qwen35_sme2_i8_tile"
mkdir -p "$build_dir"
binary="$build_dir/sme2_i8_tile_probe"

clang -O3 -std=c11 -Wall -Wextra -mcpu=native \
  tools/sme2_i8_tile_probe.c \
  -o "$binary"

{
  echo "sme2_i8_tile_probe_runner"
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  sysctl machdep.cpu.brand_string \
    hw.optional.arm.FEAT_SME \
    hw.optional.arm.FEAT_SME2 \
    hw.optional.arm.SME_I8I32 \
    hw.optional.arm.FEAT_I8MM 2>/dev/null || true
  "$binary" "$tokens" "$rows" "$k" "$iterations" "$warmup"
  echo "sme2_i8_tile_disassembly_evidence:"
  otool -tvV "$binary" 2>/dev/null \
    | grep -E 'smopa|umopa|mopa|smstart|smstop|zero[[:space:]]+\{za\}|st1w' \
    | sed 's/^/  /' || true
}
