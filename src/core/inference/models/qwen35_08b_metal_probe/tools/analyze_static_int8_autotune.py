#!/usr/bin/env python3
"""Summarize static INT8 matmul autotune output."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Candidate:
    kernel: str
    row_tile: int
    quant_group_size: int
    col_tile: int
    median_s: float
    p95_s: float
    gb_s: float


def parse(text: str) -> list[Candidate]:
    candidates: list[Candidate] = []
    current: dict[str, int | float] = {}
    for line in text.splitlines():
        header = re.match(
            r"## candidate (?:kernel=([A-Za-z0-9_]+) )?row_tile=(\d+) quant_group_size=(\d+) col_tile=(\d+)", line
        )
        if header:
            current = {
                "kernel": header.group(1) or "unknown",
                "row_tile": int(header.group(2)),
                "quant_group_size": int(header.group(3)),
                "col_tile": int(header.group(4)),
            }
            continue
        if not current:
            continue
        median = re.match(r"median_s:\s*([-+0-9.eE]+)", line)
        if median:
            current["median_s"] = float(median.group(1))
            continue
        p95 = re.match(r"p95_s:\s*([-+0-9.eE]+)", line)
        if p95:
            current["p95_s"] = float(p95.group(1))
            continue
        gb_s = re.match(r"effective_visible_gb_s:\s*([-+0-9.eE]+)", line)
        if gb_s:
            current["gb_s"] = float(gb_s.group(1))
            required = {
                "kernel",
                "row_tile",
                "quant_group_size",
                "col_tile",
                "median_s",
                "p95_s",
                "gb_s",
            }
            if required.issubset(current):
                candidates.append(
                    Candidate(
                        kernel=str(current["kernel"]),
                        row_tile=int(current["row_tile"]),
                        quant_group_size=int(current["quant_group_size"]),
                        col_tile=int(current["col_tile"]),
                        median_s=float(current["median_s"]),
                        p95_s=float(current["p95_s"]),
                        gb_s=float(current["gb_s"]),
                    )
                )
            continue
    return candidates


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--reference-median-s", type=float, default=None)
    args = parser.parse_args()
    text = args.output.read_text(encoding="utf-8")
    candidates = parse(text)

    print("static_int8_matmul_autotune_analysis")
    print(f"output: {args.output}")
    print(f"candidate_count: {len(candidates)}")
    if not candidates:
        print("status: no_candidates")
        return

    best = min(candidates, key=lambda item: item.median_s)
    worst = max(candidates, key=lambda item: item.median_s)
    print("status: ok")
    print(
        "best: "
        f"kernel={best.kernel} "
        f"row_tile={best.row_tile} "
        f"quant_group_size={best.quant_group_size} "
        f"col_tile={best.col_tile} "
        f"median_s={best.median_s:.9f} "
        f"p95_s={best.p95_s:.9f} "
        f"effective_visible_gb_s={best.gb_s:.3f}"
    )
    print(
        "worst: "
        f"kernel={worst.kernel} "
        f"row_tile={worst.row_tile} "
        f"quant_group_size={worst.quant_group_size} "
        f"median_s={worst.median_s:.9f}"
    )
    print(f"best_vs_worst_speedup: {worst.median_s / best.median_s:.3f}")
    kernels = sorted({candidate.kernel for candidate in candidates})
    for kernel in kernels:
        kernel_best = min(
            (candidate for candidate in candidates if candidate.kernel == kernel),
            key=lambda item: item.median_s,
        )
        print(
            "best_for_kernel: "
            f"kernel={kernel_best.kernel} "
            f"row_tile={kernel_best.row_tile} "
            f"quant_group_size={kernel_best.quant_group_size} "
            f"col_tile={kernel_best.col_tile} "
            f"median_s={kernel_best.median_s:.9f} "
            f"p95_s={kernel_best.p95_s:.9f} "
            f"effective_visible_gb_s={kernel_best.gb_s:.3f}"
        )
    if args.reference_median_s is not None:
        print(f"reference_median_s: {args.reference_median_s:.9f}")
        print(f"best_vs_reference_speedup: {args.reference_median_s / best.median_s:.3f}")


if __name__ == "__main__":
    main()
