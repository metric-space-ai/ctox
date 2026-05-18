#!/usr/bin/env bash
set -euo pipefail

tokens="${1:-4096}"
q_tile="${2:-128}"
k_tile="${3:-512}"
iterations="${4:-5}"
warmup="${5:-1}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
build_dir="$repo_root/target/mps-probes"
mkdir -p "$build_dir"

binary="$build_dir/tiled_attention_qk_mps_prototype"
src="$repo_root/tools/tiled_attention_qk_mps_prototype.swift"

xcrun swiftc -O \
  -framework Metal \
  -framework MetalPerformanceShaders \
  "$src" \
  -o "$binary"

exec "$binary" "$tokens" "$q_tile" "$k_tile" "$iterations" "$warmup"
