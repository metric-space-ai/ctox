#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
build_dir="$repo_root/target/mps-probes"
mkdir -p "$build_dir"

binary="$build_dir/mps_matrix_probe"
src="$repo_root/tools/mps_matrix_probe.swift"

xcrun swiftc -O \
  -framework Metal \
  -framework MetalPerformanceShaders \
  "$src" \
  -o "$binary"

exec "$binary" "$@"
