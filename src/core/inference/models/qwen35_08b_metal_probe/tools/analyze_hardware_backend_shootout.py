#!/usr/bin/env python3
"""Summarize a hardware backend shootout report."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


def metric(text: str, key: str) -> list[float]:
    out: list[float] = []
    pattern = re.compile(rf"^{re.escape(key)}:\s*([-+0-9.eE]+)\s*$", re.MULTILINE)
    for match in pattern.finditer(text):
        out.append(float(match.group(1)))
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("shootout", type=Path)
    args = parser.parse_args()
    text = args.shootout.read_text(encoding="utf-8")

    cpu_i8 = metric(text, "i8_median_s")
    cpu_q4 = metric(text, "q4_unpack_median_s")
    mps_tflops = metric(text, "effective_tflops")
    sme_tile_stream = metric(text, "stream_gb_s_best")
    sme_tile_mopa = metric(text, "mopa_per_s_best")
    coreml_status = "unknown"
    coreml_match = re.search(r'\{\s*"status":\s*"([^"]+)"', text)
    if coreml_match:
        coreml_status = coreml_match.group(1)

    print("hardware_backend_shootout_analysis")
    print(f"shootout: {args.shootout}")
    print(f"sme2_runtime_available: {'yes' if 'hw.optional.arm.FEAT_SME2=1' in text else 'unknown'}")
    print(f"sme2_compile_available: {'yes' if '__ARM_FEATURE_SME2' in text or 'sme2_compile_feature: 1' in text else 'unknown'}")
    print(f"sme2_smoke_executes: {'yes' if 'sme_streaming_call_status: ok' in text and 'sme_za_zero_status: ok' in text else 'unknown'}")
    print(f"sme2_disassembly_has_smstart: {'yes' if 'smstart' in text else 'unknown'}")
    print(f"sme2_i8_mopa_executes: {'yes' if 'sme2_mopa_probe' in text and 'status: ok' in text and 'smopa' in text else 'unknown'}")
    print(f"sme2_i8_tile_executes: {'yes' if 'sme2_i8_tile_probe' in text and 'status: ok' in text and 'smopa' in text else 'unknown'}")
    sme2_not_model_path = (
        "sme2_hotpath_status: smoke_only_not_model_path" in text
        or "sme2_usage_status: not_used_by_this_probe" in text
        or "hotpath_status: microkernel_probe_not_model_path" in text
        or "hotpath_status: tile_probe_not_model_path" in text
    )
    print(f"sme2_used_by_current_model_hotpath: {'no' if sme2_not_model_path else 'unknown'}")
    if cpu_i8:
        print(f"cpu_i8_median_s: {cpu_i8[0]:.9f}")
    if cpu_q4:
        print(f"cpu_q4_unpack_median_s: {cpu_q4[0]:.9f}")
    if mps_tflops:
        print(f"mps_effective_tflops_best: {max(mps_tflops):.3f}")
        print(f"mps_effective_tflops_all: {json.dumps(mps_tflops)}")
    if sme_tile_stream:
        print(f"sme2_i8_tile_stream_gb_s_best: {max(sme_tile_stream):.3f}")
    if sme_tile_mopa:
        print(f"sme2_i8_mopa_per_s_best: {max(sme_tile_mopa):.3f}")
    print(f"coreml_ane_status: {coreml_status}")
    if coreml_status != "measurable":
        print("coreml_ane_action: build_or_import_coreml_artifact_before_claiming_ane_use")


if __name__ == "__main__":
    main()
