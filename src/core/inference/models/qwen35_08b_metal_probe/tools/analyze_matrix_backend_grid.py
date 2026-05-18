#!/usr/bin/env python3
"""Summarize the MPS-vs-MSL matrix backend grid from a shootout report."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


SECTION_RE = re.compile(r"^###\s+(.*)$")
MEDIAN_RE = re.compile(r"^median_s:\s+([0-9.]+)")
MMA_RE = re.compile(r"^mma_median_s:\s+([0-9.]+)")
BASE_RE = re.compile(r"^baseline_median_s:\s+([0-9.]+)")


def parse_report(text: str) -> dict[str, dict[str, float]]:
    sections: dict[str, dict[str, float]] = {}
    current: str | None = None
    mps_count = 0
    for line in text.splitlines():
        section = SECTION_RE.match(line)
        if section:
            current = section.group(1).strip()
            sections[current] = {}
            continue
        if current is None:
            continue
        if match := MEDIAN_RE.match(line):
            key = "mps_median_s" if mps_count < 4 else "median_s"
            sections[current][key] = float(match.group(1))
            mps_count += 1
        elif match := MMA_RE.match(line):
            sections[current]["msl_mma_median_s"] = float(match.group(1))
        elif match := BASE_RE.match(line):
            sections[current]["msl_baseline_median_s"] = float(match.group(1))
    return sections


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shootout", type=Path)
    args = parser.parse_args()
    sections = parse_report(args.shootout.read_text())

    gate = sections.get("gate+up combined shape", {}).get("mps_median_s")
    gate_msl = sections.get("Gate/Up fallback vs MSL MMA", {}).get("msl_mma_median_s")
    down = sections.get("FFN down shape", {}).get("mps_median_s")
    down_msl = sections.get("FFN Down fallback vs MSL MMA", {}).get("msl_mma_median_s")
    delta = sections.get("Delta out shape", {}).get("mps_median_s")
    delta_msl = sections.get("Delta gated-norm + out-proj active path", {}).get("median_s")

    print("matrix_backend_grid_analysis")
    print(f"shootout: {args.shootout}")
    failures = 0
    for label, mps, msl in [
        ("gate_up_combined", gate, gate_msl),
        ("ffn_down", down, down_msl),
        ("delta_out", delta, delta_msl),
    ]:
        if mps is None or msl is None:
            print(f"{label}: missing")
            failures += 1
            continue
        ratio = msl / mps if mps > 0 else 0.0
        winner = "mps" if ratio > 1.0 else "msl"
        print(f"{label}_mps_s: {mps:.9f}")
        print(f"{label}_msl_s: {msl:.9f}")
        print(f"{label}_msl_over_mps: {ratio:.3f}")
        print(f"{label}_winner: {winner}")

    if gate and gate_msl and down and down_msl:
        print("priority: integrate_or_match_mps_for_gate_up_and_down")
    if delta and delta_msl and delta_msl < delta:
        print("keep: current_delta_out_msl_path")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
