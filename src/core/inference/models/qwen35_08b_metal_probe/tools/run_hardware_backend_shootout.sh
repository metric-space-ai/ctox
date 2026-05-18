#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/run_hardware_backend_shootout.sh <metalpack-dir> [tokens=512] [iterations=3] [output-dir]

Runs a serial hardware/backend evidence pack:
  - macOS / CPU ISA / Metal feature capture
  - CPU quant probe with NEON/I8MM/SME/SME2 compile-feature disclosure
  - MPSMatrix fp16 GEMM probes for Qwen-shaped matrix ops
  - Core ML / ANE artifact availability probe

This tool does not prove that SME2 or ANE is used by the main Metal pipeline.
It records availability and measured backend alternatives so kernel decisions
can be made from evidence.
USAGE
}

if [[ $# -lt 1 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

metalpack="$1"
tokens="${2:-512}"
iterations="${3:-3}"
out_dir="${4:-/tmp/ctox_qwen35_hardware_backend_$(date -u +%Y%m%dT%H%M%SZ)}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
mkdir -p "$out_dir"

hardware_dir="$out_dir/hardware"
tools/capture_hardware_feature_matrix.sh "$hardware_dir" >/dev/null

{
  echo "# Qwen3.5 Hardware Backend Shootout"
  echo
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "metalpack: $metalpack"
  echo "tokens: $tokens"
  echo "iterations: $iterations"
  echo "hardware_capture: $hardware_dir/hardware_feature_matrix.md"
  echo
  echo "## Hardware Summary"
  sed -n '/## Hardware/,/## CPU/p' "$hardware_dir/hardware_feature_matrix.md" | sed '$d'
  echo
  echo "## CPU ISA Feature Summary"
  grep -E 'FEAT_SME|SME_|FEAT_BF16|FEAT_I8MM|FEAT_DotProd|FEAT_FP16|__ARM_FEATURE' \
    "$hardware_dir/hardware_feature_matrix.md" || true
  echo
  echo "## CPU Quant / SIMD Probe"
  tools/run_cpu_quant_probe.sh "$tokens" 3584 1024 "$iterations" 1
  echo
  echo "## CPU SME2 Smoke Probe"
  tools/run_sme2_smoke_probe.sh
  echo
  echo "## CPU SME2 I8 MOPA Probe"
  tools/run_sme2_mopa_probe.sh 10000 "$iterations" 1
  echo
  echo "## CPU SME2 I8 Tile Stream Probe"
  tools/run_sme2_i8_tile_probe.sh "$tokens" 3584 1024 "$iterations" 1
  echo
  echo "## GPU MPSMatrix Raw GEMM Probes"
  echo
  echo "### gate/up single projection"
  tools/run_mps_matrix_probe.sh "$tokens" 3584 1024 "$iterations" 1
  echo
  echo "### gate+up combined projection"
  tools/run_mps_matrix_probe.sh "$tokens" 7168 1024 "$iterations" 1
  echo
  echo "### ffn down projection"
  tools/run_mps_matrix_probe.sh "$tokens" 1024 3584 "$iterations" 1
  echo
  echo "## Core ML / ANE Probe"
  python3 tools/coreml_ane_probe.py --repo "$repo_root" --model "$metalpack"
  echo
  echo "## Interpretation Contract"
  echo
  echo "- SME/SME2 available means CPU ISA support is present; it does not mean the Metal path uses it."
  echo "- MPSMatrix timing is GPU matrix-backend evidence; it may use M5 GPU matrix acceleration internally, but counter proof is separate."
  echo "- Core ML / ANE is only measurable when a Core ML artifact or converter path exists."
} | tee "$out_dir/shootout.md"

echo "$out_dir"
