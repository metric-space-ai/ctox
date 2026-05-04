#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:-/tmp/ctox_qwen35_hardware_$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$out_dir"

{
  echo "# CTOX Qwen3.5 Hardware Feature Capture"
  echo
  echo "captured_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host: $(hostname)"
  echo
  echo "## macOS"
  sw_vers
  echo
  echo "## Hardware"
  system_profiler SPHardwareDataType SPDisplaysDataType
  echo
  echo "## CPU / ISA Feature Sysctls"
  for key in \
    machdep.cpu.brand_string \
    hw.ncpu \
    hw.memsize \
    hw.optional.arm.FEAT_SME \
    hw.optional.arm.FEAT_SME2 \
    hw.optional.arm.FEAT_BF16 \
    hw.optional.arm.FEAT_I8MM \
    hw.optional.arm.FEAT_DotProd \
    hw.optional.arm.FEAT_FP16 \
    hw.optional.arm.FEAT_FHM \
    hw.optional.arm.FEAT_EBF16 \
    hw.optional.arm.FEAT_SME2p1 \
    hw.optional.arm.SME_F32F32 \
    hw.optional.arm.SME_BI32I32 \
    hw.optional.arm.SME_B16F32 \
    hw.optional.arm.SME_F16F32 \
    hw.optional.arm.SME_I8I32 \
    hw.optional.arm.SME_I16I32 \
    hw.optional.arm.FEAT_SME_F64F64 \
    hw.optional.arm.FEAT_SME_I16I64 \
    hw.optional.arm.FEAT_SME_F16F16 \
    hw.optional.arm.FEAT_SME_B16B16
  do
    value="$(sysctl -n "$key" 2>/dev/null || true)"
    printf '%s=%s\n' "$key" "${value:-unavailable}"
  done
  echo
  echo "## Clang ARM Compile Feature Macros"
  printf '' | clang -E -mcpu=native -dM -x c - 2>/dev/null \
    | grep -E '__ARM_FEATURE_(SME|SME2|MATMUL_INT8|BF16|DOTPROD|FP16|FHM)|__ARM_ARCH' \
    | sort || true
  echo
  echo "## Metal Probe Inventory"
  if [[ -x target/release/list_metal_counters ]]; then
    target/release/list_metal_counters || true
  elif [[ -x target/debug/list_metal_counters ]]; then
    target/debug/list_metal_counters || true
  else
    echo "list_metal_counters binary not built"
  fi
} > "$out_dir/hardware_feature_matrix.md"

cat > "$out_dir/required_kernel_gates.env" <<'EOF'
# Fill these fields before promoting a kernel on this hardware.
CTOX_HW_DEVICE_NAME=
CTOX_HW_GPU_CORES=
CTOX_HW_METAL_VERSION=
CTOX_HW_UNIFIED_MEMORY_BYTES=
CTOX_HW_UNIFIED_MEMORY_BW_GB_S=
CTOX_HW_CPU_SME=
CTOX_HW_CPU_BF16=
CTOX_HW_CPU_I8MM=
CTOX_HW_GPU_TENSOR_API_AVAILABLE=
CTOX_HW_GPU_SIMDGROUP_MATRIX_AVAILABLE=
CTOX_HW_COUNTER_HEAPS_AVAILABLE=
CTOX_HW_ROOFLINE_CAPTURE_DIR=
EOF

echo "$out_dir"
