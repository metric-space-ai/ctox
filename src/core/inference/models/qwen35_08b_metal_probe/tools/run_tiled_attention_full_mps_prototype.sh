#!/usr/bin/env bash
set -euo pipefail

tokens="${1:-4096}"
q_tile="${2:-256}"
k_tile="${3:-1024}"
iterations="${4:-3}"
warmup="${5:-1}"
heads_per_group="${6:-4}"
matrix_origins="${7:-1}"
quality_check="${8:-0}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
build_dir="$repo_root/target/swift-tools"
binary="$build_dir/tiled_attention_full_mps_prototype"
src="$repo_root/tools/tiled_attention_full_mps_prototype.swift"

mkdir -p "$build_dir"
swiftc -O "$src" -o "$binary" -framework Metal -framework MetalPerformanceShaders
"$binary" "$tokens" "$q_tile" "$k_tile" "$iterations" "$warmup" "$heads_per_group" "$matrix_origins" "$quality_check"
