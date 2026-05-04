#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <mps-ffn-sidecar-dir> [layer] [tokens] [iterations] [warmup]" >&2
  exit 2
fi

sidecar="$1"
layer="${2:-0}"
tokens="${3:-4096}"
iterations="${4:-3}"
warmup="${5:-1}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

build_dir="${TMPDIR:-/tmp}/ctox_qwen35_mps_ffn_sidecar_probe"
mkdir -p "$build_dir"
binary="$build_dir/mps_ffn_sidecar_probe"

swiftc -O \
  -framework Metal \
  -framework MetalPerformanceShaders \
  tools/mps_ffn_sidecar_probe.swift \
  -o "$binary"

"$binary" "$sidecar" "$layer" "$tokens" "$iterations" "$warmup"
