#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <metalpack-dir> [layer] [tokens] [iterations] [warmup]" >&2
  exit 2
fi

metalpack="$1"
layer="${2:-0}"
tokens="${3:-4096}"
iterations="${4:-3}"
warmup="${5:-1}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

build_dir="${TMPDIR:-/tmp}/ctox_qwen35_mps_ffn_metalpack_probe"
mkdir -p "$build_dir"
binary="$build_dir/mps_ffn_metalpack_probe"

swiftc -O \
  -framework Metal \
  -framework MetalPerformanceShaders \
  tools/mps_ffn_metalpack_probe.swift \
  -o "$binary"

"$binary" "$metalpack" "$layer" "$tokens" "$iterations" "$warmup"
