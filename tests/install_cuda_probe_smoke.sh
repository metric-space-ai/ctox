#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${1:-$repo_root/install.sh}"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/ctox-cuda-probe.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/cuda/bin" "$fixture/cuda/include" "$fixture/cuda/lib"
# Keep real installer configuration; only hardware discovery is controlled.
cuda_include_dir() { printf '%s/include\n' "$fixture/cuda"; }
cuda_library_dir() { printf '%s/lib\n' "$fixture/cuda"; }
detect_cudarc_cuda_version() { printf '12080\n'; }
(
  unset CUDA_HOME CUDA_COMPUTE_CAP
  CUDA_HOME_RESOLVED=''
  configure_cuda_env
  [[ -z "${CUDA_HOME:-}" && -z "${CUDA_COMPUTE_CAP:-}" ]]
)
(
  unset CUDA_COMPUTE_CAP
  CUDA_HOME_RESOLVED="$fixture/cuda"
  detect_cuda_compute_cap() { return 1; }
  # This must reach the assertions with errexit active even when no GPU is visible.
  configure_cuda_env
  [[ "$CUDA_HOME" == "$fixture/cuda" ]]
  [[ "$CUDA_INCLUDE_DIR" == "$fixture/cuda/include" ]]
  [[ "$CUDARC_CUDA_VERSION" == 12080 ]]
  [[ -z "${CUDA_COMPUTE_CAP:-}" ]]
)
(
  CUDA_HOME_RESOLVED="$fixture/cuda"
  detect_cuda_compute_cap() { printf '90\n'; }
  configure_cuda_env
  [[ "$CUDA_COMPUTE_CAP" == 90 ]]
)
printf 'PASS: optional CUDA capability probe preserves installer success and available configuration\n'
