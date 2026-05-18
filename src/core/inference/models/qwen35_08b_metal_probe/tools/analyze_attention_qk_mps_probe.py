#!/usr/bin/env python3
"""Summarize dense MPS QK probes for a possible tiled exact attention path."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


Q_HEADS = 8
ATTENTION_LAYERS = 6


@dataclass(frozen=True)
class Probe:
    tokens: int
    median_s: float
    tflops: float


def parse(text: str) -> list[Probe]:
    current_tokens: int | None = None
    current_median: float | None = None
    probes: list[Probe] = []
    for line in text.splitlines():
        shape = re.match(r"shape_m_n_k:\s*(\d+)\s+(\d+)\s+256", line)
        if shape and shape.group(1) == shape.group(2):
            current_tokens = int(shape.group(1))
            current_median = None
            continue
        median = re.match(r"median_s:\s*([-+0-9.eE]+)", line)
        if median:
            current_median = float(median.group(1))
            continue
        tflops = re.match(r"effective_tflops:\s*([-+0-9.eE]+)", line)
        if tflops and current_tokens is not None and current_median is not None:
            probes.append(Probe(current_tokens, current_median, float(tflops.group(1))))
            current_tokens = None
            current_median = None
    return probes


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("report", type=Path)
    args = parser.parse_args()
    probes = parse(args.report.read_text(encoding="utf-8"))

    print("attention_qk_mps_probe_analysis")
    print(f"report: {args.report}")
    print(f"candidate_count: {len(probes)}")
    print("contract: dense QK is upper-bound evidence, not a full tiled attention kernel")
    print()
    print("| tokens | qk_one_head_s | tflops | qk_8_heads_s | qk_6_layers_s |")
    print("|---:|---:|---:|---:|---:|")
    for item in probes:
        qk_8_heads = item.median_s * Q_HEADS
        qk_6_layers = qk_8_heads * ATTENTION_LAYERS
        print(
            f"| {item.tokens} | {item.median_s:.9f} | {item.tflops:.3f} | "
            f"{qk_8_heads:.9f} | {qk_6_layers:.9f} |"
        )
    print()
    print(
        "decision_hint: If exact long-prefill work continues, prototype a tiled "
        "QK-softmax-V path around this matrix backend or equivalent simdgroup "
        "matrix instructions; per-query K/V streaming is already at its byte floor."
    )


if __name__ == "__main__":
    main()
