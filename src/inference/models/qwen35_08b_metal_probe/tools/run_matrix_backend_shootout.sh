#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <metalpack-dir> [tokens] [iterations] [output-dir]" >&2
  exit 2
fi

metalpack="$1"
tokens="${2:-512}"
iterations="${3:-5}"
out_dir="${4:-/tmp/ctox_qwen35_matrix_backend_$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$out_dir"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo build --release \
  --bin bench_metalpack_prefill_gate_up_mma_compare \
  --bin bench_metalpack_prefill_down_mma_compare \
  --bin bench_metalpack_prefill_delta_out \
  >/dev/null

if [[ -f docs/kernel-dev/accepted_profile.env ]]; then
  set -a
  # shellcheck disable=SC1091
  source docs/kernel-dev/accepted_profile.env
  set +a
fi

{
  echo "# Qwen3.5 Matrix Backend Shootout"
  echo
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "metalpack: $metalpack"
  echo "tokens: $tokens"
  echo "iterations: $iterations"
  echo
  echo "## MPS Raw GEMM Probes"
  echo
  echo "### gate/up single projection shape"
  tools/run_mps_matrix_probe.sh "$tokens" 3584 1024 "$iterations" 3
  echo
  echo "### gate+up combined shape"
  tools/run_mps_matrix_probe.sh "$tokens" 7168 1024 "$iterations" 3
  echo
  echo "### FFN down shape"
  tools/run_mps_matrix_probe.sh "$tokens" 1024 3584 "$iterations" 3
  echo
  echo "### Delta out shape"
  tools/run_mps_matrix_probe.sh "$tokens" 1024 2048 "$iterations" 3
  echo
  echo "## MSL Integrated Kernel Probes"
  echo
  echo "### Gate/Up fallback vs MSL MMA"
  target/release/bench_metalpack_prefill_gate_up_mma_compare "$metalpack" 0 "$tokens" "$iterations"
  echo
  echo "### FFN Down fallback vs MSL MMA"
  target/release/bench_metalpack_prefill_down_mma_compare "$metalpack" 0 "$tokens" "$iterations"
  echo
  echo "### Delta gated-norm + out-proj active path"
  target/release/bench_metalpack_prefill_delta_out "$metalpack" 0 "$tokens" "$iterations"
} | tee "$out_dir/shootout.md"

echo "$out_dir"
