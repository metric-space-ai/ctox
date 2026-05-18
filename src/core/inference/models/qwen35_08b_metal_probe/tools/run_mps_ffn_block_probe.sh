#!/usr/bin/env bash
set -euo pipefail

tokens="${1:-4096}"
hidden="${2:-1024}"
intermediate="${3:-3584}"
iterations="${4:-5}"
warmup="${5:-2}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

build_dir="${TMPDIR:-/tmp}/ctox_qwen35_mps_ffn_probe"
mkdir -p "$build_dir"
binary="$build_dir/mps_ffn_block_probe"

swiftc -O \
  -framework Metal \
  -framework MetalPerformanceShaders \
  tools/mps_ffn_block_probe.swift \
  -o "$binary"

"$binary" "$tokens" "$hidden" "$intermediate" "$iterations" "$warmup"
