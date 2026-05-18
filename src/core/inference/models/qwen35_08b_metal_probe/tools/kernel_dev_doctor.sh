#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage: tools/kernel_dev_doctor.sh [--strict-experiments]

Checks the kernel-dev knowledge/tooling layer:
  - required handbook/wiki/template files exist
  - shell tools parse with bash -n
  - README / handbook link the core tools
  - generated experiment, measurement, and decision records validate in default mode
  - generated cache forensics records validate in default mode
  - generated autotune records validate in default mode
  - generated accepted-profile update proposals validate in default mode
  - optionally validate generated records in strict mode

This script does not run performance benchmarks.
USAGE
}

strict_experiments=0
if [[ "${1:-}" == "--strict-experiments" ]]; then
  strict_experiments=1
  shift
fi

if [[ $# -ne 0 || "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

failures=()
warnings=()

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    failures+=("missing required file: $path")
  fi
}

require_grep() {
  local pattern="$1"
  local path="$2"
  if ! grep -q -- "$pattern" "$path"; then
    failures+=("missing '$pattern' in $path")
  fi
}

required_files=(
  README.md
  RESEARCH_LOG.md
  KERNEL_DEV_HANDBOOK.md
  docs/kernel-dev/README.md
  docs/kernel-dev/EXPERIMENT_TEMPLATE.md
  docs/kernel-dev/DECISION_RECORD_TEMPLATE.md
  docs/kernel-dev/FORENSICS_RECORD_TEMPLATE.md
  docs/kernel-dev/HARDWARE_BACKEND_GRID.md
  docs/kernel-dev/AUTOTUNE_RECORD_TEMPLATE.md
  docs/kernel-dev/ACCEPTED_PROFILE_UPDATE_TEMPLATE.md
  docs/kernel-dev/MEASUREMENT_RECORD_TEMPLATE.md
  docs/kernel-dev/BENCHMARK_PROTOCOL.md
  docs/kernel-dev/CACHE_FORENSICS_CHECKLIST.md
  docs/kernel-dev/FLAG_LIFECYCLE_TEMPLATE.md
  docs/kernel-dev/accepted_profile.env
  docs/kernel-dev/experiments/README.md
  docs/kernel-dev/experiments/INDEX.md
  docs/kernel-dev/decisions/README.md
  docs/kernel-dev/decisions/INDEX.md
  docs/kernel-dev/forensics/README.md
  docs/kernel-dev/forensics/INDEX.md
  docs/kernel-dev/autotune/README.md
  docs/kernel-dev/autotune/INDEX.md
  docs/kernel-dev/profile-updates/README.md
  docs/kernel-dev/profile-updates/INDEX.md
  docs/kernel-dev/measurements/README.md
  docs/kernel-dev/measurements/INDEX.md
  tools/new_kernel_experiment.sh
  tools/new_kernel_decision.sh
  tools/validate_kernel_decision.sh
  tools/check_kernel_promotion.sh
  tools/validate_kernel_experiment.sh
  tools/new_cache_forensics_record.sh
  tools/validate_cache_forensics.sh
  tools/fill_forensics_record_from_output.sh
  tools/analyze_bandwidth_gap.sh
  tools/analyze_memory_forensics_gaps.sh
  tools/analyze_delta_profile_gaps.sh
  tools/list_cache_forensics.sh
  tools/update_cache_forensics_index.sh
  tools/new_autotune_record.sh
  tools/validate_autotune_record.sh
  tools/list_autotune_records.sh
  tools/update_autotune_index.sh
  tools/normalize_benchmark_output.sh
  tools/fill_autotune_record_from_output.sh
  tools/capture_measurement_output.sh
  tools/new_measurement_record.sh
  tools/validate_measurement_record.sh
  tools/list_measurement_records.sh
  tools/update_measurement_index.sh
  tools/show_kernel_evidence_bundle.sh
  tools/propose_accepted_profile_update.sh
  tools/validate_accepted_profile_update.sh
  tools/validate_accepted_profile.sh
  tools/check_autotune_defaults.sh
  tools/list_accepted_profile_updates.sh
  tools/update_accepted_profile_update_index.sh
  tools/kernel_dev_doctor.sh
  tools/run_accepted_profile.sh
  tools/run_measurement_pack.sh
  tools/validate_quant_pipeline.py
  tools/validate_metalpack_quant_manifest.py
  tools/analyze_matrix_backend_grid.py
  tools/analyze_hardware_backend_shootout.py
  tools/analyze_static_int8_autotune.py
  tools/prefill_reference_report.py
  tools/run_decode_regression_matrix.sh
  tools/exact_attention_traffic_report.py
  tools/run_attention_qk_mps_probe.sh
  tools/analyze_attention_qk_mps_probe.py
  tools/plan_tiled_attention.py
  tools/tiled_attention_qk_mps_prototype.swift
  tools/run_tiled_attention_qk_mps_prototype.sh
  tools/run_tiled_attention_qk_mps_grid.sh
  tools/analyze_tiled_attention_qk_mps_grid.py
  tools/tiled_attention_full_mps_prototype.swift
  tools/run_tiled_attention_full_mps_prototype.sh
  tools/run_prefill_attention_backend_matrix.sh
  tools/run_hardware_backend_shootout.sh
  tools/run_sme2_smoke_probe.sh
  tools/run_sme2_mopa_probe.sh
  tools/run_sme2_i8_tile_probe.sh
  tools/run_static_int8_matmul_autotune.sh
  tools/run_cpu_quant_probe.sh
  tools/run_mps_ffn_block_probe.sh
  tools/run_mps_ffn_metalpack_probe.sh
  tools/run_mps_ffn_sidecar_probe.sh
  tools/run_mps_deltanet_project_probe.sh
  tools/run_mps_deltanet_project_sidecar_probe.sh
  tools/estimate_mps_ffn_prefill_impact.py
  tools/compare_delta_stack_candidate.sh
  tools/capture_roofline_baseline.sh
  tools/list_kernel_experiments.sh
  tools/update_kernel_experiment_index.sh
  tools/list_kernel_decisions.sh
  tools/update_kernel_decision_index.sh
)

for path in "${required_files[@]}"; do
  require_file "$path"
done

for script in tools/*.sh; do
  [[ -f "$script" ]] || continue
  if ! bash -n "$script"; then
    failures+=("bash syntax check failed: $script")
  fi
done

if [[ -f README.md ]]; then
  require_grep "KERNEL_DEV_HANDBOOK.md" README.md
  require_grep "docs/kernel-dev" README.md
  require_grep "tools/new_kernel_experiment.sh" README.md
  require_grep "tools/validate_kernel_experiment.sh" README.md
  require_grep "tools/run_accepted_profile.sh" README.md
  require_grep "tools/run_measurement_pack.sh" README.md
  require_grep "tools/compare_delta_stack_candidate.sh" README.md
  require_grep "tools/capture_roofline_baseline.sh" README.md
  require_grep "tools/list_kernel_experiments.sh" README.md
  require_grep "tools/update_kernel_experiment_index.sh" README.md
  require_grep "tools/new_kernel_decision.sh" README.md
  require_grep "tools/validate_kernel_decision.sh" README.md
  require_grep "tools/check_kernel_promotion.sh" README.md
  require_grep "tools/new_cache_forensics_record.sh" README.md
  require_grep "tools/validate_cache_forensics.sh" README.md
  require_grep "tools/fill_forensics_record_from_output.sh" README.md
  require_grep "tools/analyze_bandwidth_gap.sh" README.md
  require_grep "tools/analyze_memory_forensics_gaps.sh" README.md
  require_grep "tools/analyze_delta_profile_gaps.sh" README.md
  require_grep "tools/new_autotune_record.sh" README.md
  require_grep "tools/validate_autotune_record.sh" README.md
  require_grep "tools/normalize_benchmark_output.sh" README.md
  require_grep "tools/capture_measurement_output.sh" README.md
  require_grep "tools/new_measurement_record.sh" README.md
  require_grep "tools/show_kernel_evidence_bundle.sh" README.md
  require_grep "tools/propose_accepted_profile_update.sh" README.md
  require_grep "tools/validate_accepted_profile.sh" README.md
  require_grep "tools/check_autotune_defaults.sh" README.md
  require_grep "tools/run_decode_regression_matrix.sh" README.md
  require_grep "tools/run_prefill_attention_backend_matrix.sh" README.md
fi

if [[ -f KERNEL_DEV_HANDBOOK.md ]]; then
  require_grep "tools/new_kernel_experiment.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/validate_kernel_experiment.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "FLAG_LIFECYCLE_TEMPLATE.md" KERNEL_DEV_HANDBOOK.md
  require_grep "accepted_profile.env" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_measurement_pack.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/validate_quant_pipeline.py" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/validate_metalpack_quant_manifest.py" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/analyze_matrix_backend_grid.py" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/analyze_hardware_backend_shootout.py" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_hardware_backend_shootout.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_sme2_smoke_probe.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_sme2_mopa_probe.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_cpu_quant_probe.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_mps_ffn_block_probe.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_mps_ffn_metalpack_probe.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_mps_ffn_sidecar_probe.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "pack_mps_ffn_sidecar" KERNEL_DEV_HANDBOOK.md
  require_grep "bench_mps_ffn_sidecar_runtime" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_mps_deltanet_project_probe.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_mps_deltanet_project_sidecar_probe.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "pack_mps_delta_project_sidecar" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/estimate_mps_ffn_prefill_impact.py" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/compare_delta_stack_candidate.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/capture_roofline_baseline.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/list_kernel_experiments.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/update_kernel_experiment_index.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/new_kernel_decision.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/validate_kernel_decision.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/check_kernel_promotion.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/new_cache_forensics_record.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/validate_cache_forensics.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/fill_forensics_record_from_output.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/analyze_bandwidth_gap.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/analyze_memory_forensics_gaps.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/analyze_delta_profile_gaps.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/new_autotune_record.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/validate_autotune_record.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/normalize_benchmark_output.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/capture_measurement_output.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/new_measurement_record.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/show_kernel_evidence_bundle.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/propose_accepted_profile_update.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/validate_accepted_profile.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/check_autotune_defaults.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_decode_regression_matrix.sh" KERNEL_DEV_HANDBOOK.md
  require_grep "tools/run_prefill_attention_backend_matrix.sh" KERNEL_DEV_HANDBOOK.md
fi

if [[ -f docs/kernel-dev/README.md ]]; then
  require_grep "EXPERIMENT_TEMPLATE.md" docs/kernel-dev/README.md
  require_grep "BENCHMARK_PROTOCOL.md" docs/kernel-dev/README.md
  require_grep "CACHE_FORENSICS_CHECKLIST.md" docs/kernel-dev/README.md
  require_grep "FLAG_LIFECYCLE_TEMPLATE.md" docs/kernel-dev/README.md
  require_grep "accepted_profile.env" docs/kernel-dev/README.md
  require_grep "validate_kernel_decision.sh" docs/kernel-dev/README.md
  require_grep "capture_roofline_baseline.sh" docs/kernel-dev/README.md
  require_grep "check_kernel_promotion.sh" docs/kernel-dev/README.md
  require_grep "FORENSICS_RECORD_TEMPLATE.md" docs/kernel-dev/README.md
  require_grep "validate_cache_forensics.sh" docs/kernel-dev/README.md
  require_grep "fill_forensics_record_from_output.sh" docs/kernel-dev/README.md
  require_grep "analyze_bandwidth_gap.sh" docs/kernel-dev/README.md
  require_grep "analyze_memory_forensics_gaps.sh" docs/kernel-dev/README.md
  require_grep "analyze_delta_profile_gaps.sh" docs/kernel-dev/README.md
  require_grep "AUTOTUNE_RECORD_TEMPLATE.md" docs/kernel-dev/README.md
  require_grep "validate_autotune_record.sh" docs/kernel-dev/README.md
  require_grep "normalize_benchmark_output.sh" docs/kernel-dev/README.md
  require_grep "capture_measurement_output.sh" docs/kernel-dev/README.md
  require_grep "compare_delta_stack_candidate.sh" docs/kernel-dev/README.md
  require_grep "MEASUREMENT_RECORD_TEMPLATE.md" docs/kernel-dev/README.md
  require_grep "new_measurement_record.sh" docs/kernel-dev/README.md
  require_grep "show_kernel_evidence_bundle.sh" docs/kernel-dev/README.md
  require_grep "ACCEPTED_PROFILE_UPDATE_TEMPLATE.md" docs/kernel-dev/README.md
  require_grep "propose_accepted_profile_update.sh" docs/kernel-dev/README.md
  require_grep "validate_accepted_profile.sh" docs/kernel-dev/README.md
  require_grep "check_autotune_defaults.sh" docs/kernel-dev/README.md
  require_grep "validate_quant_pipeline.py" docs/kernel-dev/README.md
  require_grep "validate_metalpack_quant_manifest.py" docs/kernel-dev/README.md
  require_grep "analyze_matrix_backend_grid.py" docs/kernel-dev/README.md
  require_grep "analyze_hardware_backend_shootout.py" docs/kernel-dev/README.md
  require_grep "analyze_static_int8_autotune.py" docs/kernel-dev/README.md
  require_grep "prefill_reference_report.py" docs/kernel-dev/README.md
  require_grep "run_decode_regression_matrix.sh" docs/kernel-dev/README.md
  require_grep "exact_attention_traffic_report.py" docs/kernel-dev/README.md
  require_grep "run_attention_qk_mps_probe.sh" docs/kernel-dev/README.md
  require_grep "analyze_attention_qk_mps_probe.py" docs/kernel-dev/README.md
  require_grep "plan_tiled_attention.py" docs/kernel-dev/README.md
  require_grep "run_tiled_attention_qk_mps_prototype.sh" docs/kernel-dev/README.md
  require_grep "run_tiled_attention_qk_mps_grid.sh" docs/kernel-dev/README.md
  require_grep "analyze_tiled_attention_qk_mps_grid.py" docs/kernel-dev/README.md
  require_grep "run_tiled_attention_full_mps_prototype.sh" docs/kernel-dev/README.md
  require_grep "run_prefill_attention_backend_matrix.sh" docs/kernel-dev/README.md
  require_grep "run_hardware_backend_shootout.sh" docs/kernel-dev/README.md
  require_grep "run_sme2_smoke_probe.sh" docs/kernel-dev/README.md
  require_grep "run_sme2_mopa_probe.sh" docs/kernel-dev/README.md
  require_grep "run_sme2_i8_tile_probe.sh" docs/kernel-dev/README.md
  require_grep "run_static_int8_matmul_autotune.sh" docs/kernel-dev/README.md
  require_grep "run_cpu_quant_probe.sh" docs/kernel-dev/README.md
  require_grep "run_mps_ffn_block_probe.sh" docs/kernel-dev/README.md
  require_grep "run_mps_ffn_metalpack_probe.sh" docs/kernel-dev/README.md
  require_grep "run_mps_ffn_sidecar_probe.sh" docs/kernel-dev/README.md
  require_grep "pack_mps_ffn_sidecar" docs/kernel-dev/README.md
  require_grep "bench_mps_ffn_sidecar_runtime" docs/kernel-dev/README.md
  require_grep "run_mps_deltanet_project_probe.sh" docs/kernel-dev/README.md
  require_grep "run_mps_deltanet_project_sidecar_probe.sh" docs/kernel-dev/README.md
  require_grep "pack_mps_delta_project_sidecar" docs/kernel-dev/README.md
  require_grep "estimate_mps_ffn_prefill_impact.py" docs/kernel-dev/README.md
fi

if [[ -f docs/kernel-dev/accepted_profile.env && -x tools/validate_accepted_profile.sh ]]; then
  if ! tools/validate_accepted_profile.sh docs/kernel-dev/accepted_profile.env >/tmp/ctox_kernel_doctor_accepted_profile.$$ 2>&1; then
    failures+=("accepted profile validation failed: docs/kernel-dev/accepted_profile.env")
    while IFS= read -r line; do
      failures+=("  $line")
    done </tmp/ctox_kernel_doctor_accepted_profile.$$
  fi
  rm -f /tmp/ctox_kernel_doctor_accepted_profile.$$
fi

if [[ -f docs/kernel-dev/accepted_profile.env && -x tools/check_autotune_defaults.sh && -x target/release/autotune_metalpack_prefill_delta_stack ]]; then
  if ! tools/check_autotune_defaults.sh docs/kernel-dev/accepted_profile.env >/tmp/ctox_kernel_doctor_autotune_defaults.$$ 2>&1; then
    failures+=("autotune default validation failed against accepted profile")
    while IFS= read -r line; do
      failures+=("  $line")
    done </tmp/ctox_kernel_doctor_autotune_defaults.$$
  fi
  rm -f /tmp/ctox_kernel_doctor_autotune_defaults.$$
fi

measurement_count=0
measurement_validated_count=0
measurement_strict_failed_count=0
if [[ -d docs/kernel-dev/measurements ]]; then
  while IFS= read -r record; do
    [[ "$(basename "$record")" == "README.md" ]] && continue
    [[ "$(basename "$record")" == "INDEX.md" ]] && continue
    measurement_count=$((measurement_count + 1))
    if tools/validate_measurement_record.sh "$record" >/tmp/ctox_kernel_doctor_measurement_validate.$$ 2>&1; then
      measurement_validated_count=$((measurement_validated_count + 1))
    else
      failures+=("measurement default validation failed: $record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_kernel_doctor_measurement_validate.$$
    fi
    rm -f /tmp/ctox_kernel_doctor_measurement_validate.$$

    if [[ "$strict_experiments" -eq 1 ]]; then
      if ! tools/validate_measurement_record.sh --strict "$record" >/tmp/ctox_kernel_doctor_measurement_strict.$$ 2>&1; then
        measurement_strict_failed_count=$((measurement_strict_failed_count + 1))
        warnings+=("measurement strict validation failed: $record")
      fi
      rm -f /tmp/ctox_kernel_doctor_measurement_strict.$$
    fi
  done < <(find docs/kernel-dev/measurements -maxdepth 1 -type f -name '*.md' | sort)
fi

if [[ -f docs/kernel-dev/measurements/INDEX.md && -x tools/list_measurement_records.sh ]]; then
  tmp_measurement_index="$(mktemp /tmp/ctox_measurement_index.XXXXXX.md)"
  {
    cat <<'HEADER'
# Measurement Index

Generated from `tools/list_measurement_records.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_measurement_index.sh
```

HEADER
    tools/list_measurement_records.sh --markdown
  } > "$tmp_measurement_index"
  if ! cmp -s "$tmp_measurement_index" docs/kernel-dev/measurements/INDEX.md; then
    failures+=("measurement index is stale; run tools/update_measurement_index.sh")
  fi
  rm -f "$tmp_measurement_index"
fi

experiment_count=0
validated_count=0
strict_failed_count=0
if [[ -d docs/kernel-dev/experiments ]]; then
  while IFS= read -r record; do
    [[ "$(basename "$record")" == "README.md" ]] && continue
    [[ "$(basename "$record")" == "INDEX.md" ]] && continue
    experiment_count=$((experiment_count + 1))
    if tools/validate_kernel_experiment.sh "$record" >/tmp/ctox_kernel_doctor_validate.$$ 2>&1; then
      validated_count=$((validated_count + 1))
    else
      failures+=("experiment default validation failed: $record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_kernel_doctor_validate.$$
    fi
    rm -f /tmp/ctox_kernel_doctor_validate.$$

    if [[ "$strict_experiments" -eq 1 ]]; then
      if ! tools/validate_kernel_experiment.sh --strict "$record" >/tmp/ctox_kernel_doctor_strict.$$ 2>&1; then
        strict_failed_count=$((strict_failed_count + 1))
        warnings+=("experiment strict validation failed: $record")
      fi
      rm -f /tmp/ctox_kernel_doctor_strict.$$
    fi
  done < <(find docs/kernel-dev/experiments -maxdepth 1 -type f -name '*.md' | sort)
fi

if [[ -f docs/kernel-dev/experiments/INDEX.md && -x tools/list_kernel_experiments.sh ]]; then
  tmp_index="$(mktemp /tmp/ctox_kernel_experiment_index.XXXXXX.md)"
  {
    cat <<'HEADER'
# Kernel Experiment Index

Generated from `tools/list_kernel_experiments.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_kernel_experiment_index.sh
```

HEADER
    tools/list_kernel_experiments.sh --markdown
  } > "$tmp_index"
  if ! cmp -s "$tmp_index" docs/kernel-dev/experiments/INDEX.md; then
    failures+=("experiment index is stale; run tools/update_kernel_experiment_index.sh")
  fi
  rm -f "$tmp_index"
fi

decision_count=0
decision_validated_count=0
decision_strict_failed_count=0
if [[ -d docs/kernel-dev/decisions ]]; then
  while IFS= read -r record; do
    [[ "$(basename "$record")" == "README.md" ]] && continue
    [[ "$(basename "$record")" == "INDEX.md" ]] && continue
    decision_count=$((decision_count + 1))
    if tools/validate_kernel_decision.sh "$record" >/tmp/ctox_kernel_doctor_decision_validate.$$ 2>&1; then
      decision_validated_count=$((decision_validated_count + 1))
    else
      failures+=("decision default validation failed: $record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_kernel_doctor_decision_validate.$$
    fi
    rm -f /tmp/ctox_kernel_doctor_decision_validate.$$

    if [[ "$strict_experiments" -eq 1 ]]; then
      if ! tools/validate_kernel_decision.sh --strict "$record" >/tmp/ctox_kernel_doctor_decision_strict.$$ 2>&1; then
        decision_strict_failed_count=$((decision_strict_failed_count + 1))
        warnings+=("decision strict validation failed: $record")
      fi
      rm -f /tmp/ctox_kernel_doctor_decision_strict.$$
    fi
  done < <(find docs/kernel-dev/decisions -maxdepth 1 -type f -name '*.md' | sort)
fi

if [[ -f docs/kernel-dev/decisions/INDEX.md && -x tools/list_kernel_decisions.sh ]]; then
  tmp_decision_index="$(mktemp /tmp/ctox_kernel_decision_index.XXXXXX.md)"
  {
    cat <<'HEADER'
# Kernel Decision Index

Generated from `tools/list_kernel_decisions.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_kernel_decision_index.sh
```

HEADER
    tools/list_kernel_decisions.sh --markdown
  } > "$tmp_decision_index"
  if ! cmp -s "$tmp_decision_index" docs/kernel-dev/decisions/INDEX.md; then
    failures+=("decision index is stale; run tools/update_kernel_decision_index.sh")
  fi
  rm -f "$tmp_decision_index"
fi

forensics_count=0
forensics_validated_count=0
forensics_strict_failed_count=0
if [[ -d docs/kernel-dev/forensics ]]; then
  while IFS= read -r record; do
    [[ "$(basename "$record")" == "README.md" ]] && continue
    [[ "$(basename "$record")" == "INDEX.md" ]] && continue
    forensics_count=$((forensics_count + 1))
    if tools/validate_cache_forensics.sh "$record" >/tmp/ctox_kernel_doctor_forensics_validate.$$ 2>&1; then
      forensics_validated_count=$((forensics_validated_count + 1))
    else
      failures+=("forensics default validation failed: $record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_kernel_doctor_forensics_validate.$$
    fi
    rm -f /tmp/ctox_kernel_doctor_forensics_validate.$$

    if [[ "$strict_experiments" -eq 1 ]]; then
      if ! tools/validate_cache_forensics.sh --strict "$record" >/tmp/ctox_kernel_doctor_forensics_strict.$$ 2>&1; then
        forensics_strict_failed_count=$((forensics_strict_failed_count + 1))
        warnings+=("forensics strict validation failed: $record")
      fi
      rm -f /tmp/ctox_kernel_doctor_forensics_strict.$$
    fi
  done < <(find docs/kernel-dev/forensics -maxdepth 1 -type f -name '*.md' | sort)
fi

if [[ -f docs/kernel-dev/forensics/INDEX.md && -x tools/list_cache_forensics.sh ]]; then
  tmp_forensics_index="$(mktemp /tmp/ctox_cache_forensics_index.XXXXXX.md)"
  {
    cat <<'HEADER'
# Cache Forensics Index

Generated from `tools/list_cache_forensics.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_cache_forensics_index.sh
```

HEADER
    tools/list_cache_forensics.sh --markdown
  } > "$tmp_forensics_index"
  if ! cmp -s "$tmp_forensics_index" docs/kernel-dev/forensics/INDEX.md; then
    failures+=("cache forensics index is stale; run tools/update_cache_forensics_index.sh")
  fi
  rm -f "$tmp_forensics_index"
fi

autotune_count=0
autotune_validated_count=0
autotune_strict_failed_count=0
if [[ -d docs/kernel-dev/autotune ]]; then
  while IFS= read -r record; do
    [[ "$(basename "$record")" == "README.md" ]] && continue
    [[ "$(basename "$record")" == "INDEX.md" ]] && continue
    autotune_count=$((autotune_count + 1))
    if tools/validate_autotune_record.sh "$record" >/tmp/ctox_kernel_doctor_autotune_validate.$$ 2>&1; then
      autotune_validated_count=$((autotune_validated_count + 1))
    else
      failures+=("autotune default validation failed: $record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_kernel_doctor_autotune_validate.$$
    fi
    rm -f /tmp/ctox_kernel_doctor_autotune_validate.$$

    if [[ "$strict_experiments" -eq 1 ]]; then
      if ! tools/validate_autotune_record.sh --strict "$record" >/tmp/ctox_kernel_doctor_autotune_strict.$$ 2>&1; then
        autotune_strict_failed_count=$((autotune_strict_failed_count + 1))
        warnings+=("autotune strict validation failed: $record")
      fi
      rm -f /tmp/ctox_kernel_doctor_autotune_strict.$$
    fi
  done < <(find docs/kernel-dev/autotune -maxdepth 1 -type f -name '*.md' | sort)
fi

if [[ -f docs/kernel-dev/autotune/INDEX.md && -x tools/list_autotune_records.sh ]]; then
  tmp_autotune_index="$(mktemp /tmp/ctox_autotune_index.XXXXXX.md)"
  {
    cat <<'HEADER'
# Autotune Index

Generated from `tools/list_autotune_records.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_autotune_index.sh
```

HEADER
    tools/list_autotune_records.sh --markdown
  } > "$tmp_autotune_index"
  if ! cmp -s "$tmp_autotune_index" docs/kernel-dev/autotune/INDEX.md; then
    failures+=("autotune index is stale; run tools/update_autotune_index.sh")
  fi
  rm -f "$tmp_autotune_index"
fi

profile_update_count=0
profile_update_validated_count=0
profile_update_strict_failed_count=0
if [[ -d docs/kernel-dev/profile-updates ]]; then
  while IFS= read -r record; do
    [[ "$(basename "$record")" == "README.md" ]] && continue
    [[ "$(basename "$record")" == "INDEX.md" ]] && continue
    profile_update_count=$((profile_update_count + 1))
    if tools/validate_accepted_profile_update.sh "$record" >/tmp/ctox_kernel_doctor_profile_update_validate.$$ 2>&1; then
      profile_update_validated_count=$((profile_update_validated_count + 1))
    else
      failures+=("profile update default validation failed: $record")
      while IFS= read -r line; do
        failures+=("  $line")
      done </tmp/ctox_kernel_doctor_profile_update_validate.$$
    fi
    rm -f /tmp/ctox_kernel_doctor_profile_update_validate.$$

    if [[ "$strict_experiments" -eq 1 ]]; then
      if ! tools/validate_accepted_profile_update.sh --strict "$record" >/tmp/ctox_kernel_doctor_profile_update_strict.$$ 2>&1; then
        profile_update_strict_failed_count=$((profile_update_strict_failed_count + 1))
        warnings+=("profile update strict validation failed: $record")
      fi
      rm -f /tmp/ctox_kernel_doctor_profile_update_strict.$$
    fi
  done < <(find docs/kernel-dev/profile-updates -maxdepth 1 -type f -name '*.md' | sort)
fi

if [[ -f docs/kernel-dev/profile-updates/INDEX.md && -x tools/list_accepted_profile_updates.sh ]]; then
  tmp_profile_update_index="$(mktemp /tmp/ctox_profile_update_index.XXXXXX.md)"
  {
    cat <<'HEADER'
# Accepted Profile Update Index

Generated from `tools/list_accepted_profile_updates.sh --markdown`.

Do not edit the table manually. Regenerate with:

```text
tools/update_accepted_profile_update_index.sh
```

HEADER
    tools/list_accepted_profile_updates.sh --markdown
  } > "$tmp_profile_update_index"
  if ! cmp -s "$tmp_profile_update_index" docs/kernel-dev/profile-updates/INDEX.md; then
    failures+=("accepted-profile update index is stale; run tools/update_accepted_profile_update_index.sh")
  fi
  rm -f "$tmp_profile_update_index"
fi

echo "kernel-dev doctor"
echo "required_files: ${#required_files[@]}"
echo "experiments: $experiment_count"
echo "experiments_valid_default: $validated_count"
echo "measurements: $measurement_count"
echo "measurements_valid_default: $measurement_validated_count"
echo "decisions: $decision_count"
echo "decisions_valid_default: $decision_validated_count"
echo "forensics: $forensics_count"
echo "forensics_valid_default: $forensics_validated_count"
echo "autotune: $autotune_count"
echo "autotune_valid_default: $autotune_validated_count"
echo "profile_updates: $profile_update_count"
echo "profile_updates_valid_default: $profile_update_validated_count"
if [[ "$strict_experiments" -eq 1 ]]; then
  echo "experiments_failed_strict: $strict_failed_count"
  echo "measurements_failed_strict: $measurement_strict_failed_count"
  echo "decisions_failed_strict: $decision_strict_failed_count"
  echo "forensics_failed_strict: $forensics_strict_failed_count"
  echo "autotune_failed_strict: $autotune_strict_failed_count"
  echo "profile_updates_failed_strict: $profile_update_strict_failed_count"
fi

if [[ "${#warnings[@]}" -gt 0 ]]; then
  echo "warnings:"
  for warning in "${warnings[@]}"; do
    echo "  - $warning"
  done
fi

if [[ "${#failures[@]}" -gt 0 ]]; then
  echo "validation: FAIL"
  for failure in "${failures[@]}"; do
    echo "  - $failure"
  done
  exit 1
fi

echo "validation: PASS"
