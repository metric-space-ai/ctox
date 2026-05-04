#!/usr/bin/env bash
set -euo pipefail

tokens="${1:-128}"
rows="${2:-3584}"
k="${3:-1024}"
iterations="${4:-5}"
warmup="${5:-2}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

build_dir="${TMPDIR:-/tmp}/ctox_qwen35_cpu_quant_probe"
mkdir -p "$build_dir"
binary="$build_dir/cpu_quant_probe"

clang -O3 -std=c11 -Wall -Wextra -mcpu=native \
  tools/cpu_quant_probe.c \
  -o "$binary"

{
  echo "cpu_quant_probe_runner"
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  sysctl machdep.cpu.brand_string hw.optional.arm.FEAT_SME hw.optional.arm.FEAT_SME2 \
    hw.optional.arm.FEAT_BF16 hw.optional.arm.FEAT_I8MM hw.optional.arm.FEAT_DotProd \
    hw.optional.arm.FEAT_FP16 hw.optional.arm.FEAT_FHM 2>/dev/null || true
  "$binary" "$tokens" "$rows" "$k" "$iterations" "$warmup"
}
