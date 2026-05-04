#!/usr/bin/env bash
set -euo pipefail

tokens="${1:-4096}"
hidden="${2:-1024}"
qkv_rows="${3:-6144}"
z_rows="${4:-2048}"
iterations="${5:-3}"
warmup="${6:-1}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

build_dir="${TMPDIR:-/tmp}/ctox_qwen35_mps_delta_project_probe"
mkdir -p "$build_dir"
binary="$build_dir/mps_deltanet_project_probe"

swiftc -O \
  -framework Metal \
  -framework MetalPerformanceShaders \
  tools/mps_deltanet_project_probe.swift \
  -o "$binary"

"$binary" "$tokens" "$hidden" "$qkv_rows" "$z_rows" "$iterations" "$warmup"
