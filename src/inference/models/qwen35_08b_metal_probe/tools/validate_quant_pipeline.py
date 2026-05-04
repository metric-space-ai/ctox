#!/usr/bin/env python3
"""Validate a quantized pipeline record for hot-loop conversion hazards."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


REQUIRED = [
    "target_compute_backend",
    "hardware_quant_reason",
    "hardware_feature_evidence",
    "source_checkpoint_dtype",
    "packed_storage_dtype",
    "runtime_input_dtype",
    "runtime_accumulator_dtype",
    "runtime_output_dtype",
    "state_or_cache_dtype",
    "quantization_time",
    "dequantization_policy",
    "repack_policy",
    "layout_order",
    "group_stride_bytes",
    "prefetch_or_speculation_contract",
    "f32_to_f16_to_f32_hot_loop",
    "materialized_dequant_tensor",
    "per_token_requantization",
    "per_dispatch_repacking",
]

KNOWN_BACKENDS = {
    "gpu_msl_simdgroup",
    "gpu_mps_matrix",
    "gpu_metal4_tensor",
    "cpu_neon",
    "cpu_sme",
    "coreml_ane",
    "hybrid",
    "unknown",
}

FORBIDDEN_YES = [
    "f32_to_f16_to_f32_hot_loop",
    "materialized_dequant_tensor",
    "per_token_requantization",
    "per_dispatch_repacking",
]


def parse_fields(text: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for line in text.splitlines():
        match = re.match(r"^\s*([A-Za-z0-9_.-]+)\s*:\s*(.*?)\s*$", line)
        if match:
            fields[match.group(1)] = match.group(2)
    return fields


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("record", type=Path)
    parser.add_argument("--strict", action="store_true")
    args = parser.parse_args()

    fields = parse_fields(args.record.read_text(encoding="utf-8"))
    failures: list[str] = []

    for key in REQUIRED:
        if not fields.get(key):
            failures.append(f"missing required field `{key}`")

    backend = fields.get("target_compute_backend")
    if backend and backend not in KNOWN_BACKENDS:
        failures.append(f"unknown target_compute_backend `{backend}`")

    for key in FORBIDDEN_YES:
        if fields.get(key, "").lower() == "yes":
            failures.append(f"hot-loop conversion hazard `{key}` is yes")

    if args.strict and fields.get("target_compute_backend") == "unknown":
        failures.append("strict mode requires a concrete target_compute_backend")

    if args.strict and fields.get("hardware_feature_evidence", "").lower() in {"", "n/a", "unknown"}:
        failures.append("strict mode requires hardware_feature_evidence")

    if args.strict and fields.get("prefetch_or_speculation_contract", "").lower() in {"", "n/a", "unknown"}:
        failures.append("strict mode requires prefetch_or_speculation_contract")

    if fields.get("quantization_time") in {"per-token", "per-dispatch"}:
        failures.append("quantization_time must not be per-token or per-dispatch")

    if fields.get("repack_policy") in {"per-token", "per-dispatch"}:
        failures.append("repack_policy must not be per-token or per-dispatch")

    if args.strict and fields.get("dequantization_policy") == "materialize-full-tensor":
        failures.append("strict mode forbids materialized full dequant tensors")

    print("quant_pipeline_validation")
    print(f"record: {args.record}")
    if failures:
        print("validation: FAIL")
        for failure in failures:
            print(f"failure: {failure}")
        return 1
    print("validation: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
