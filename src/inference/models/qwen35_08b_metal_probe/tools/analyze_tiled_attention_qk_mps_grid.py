#!/usr/bin/env python3
"""Analyze tiled QK MPS prototype grid output."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Candidate:
    q_tile: int
    k_tile: int
    pairs: int
    median_s: float
    tflops: float
    encodes_per_s: float


def parse(text: str) -> list[Candidate]:
    current: dict[str, int | float] = {}
    candidates: list[Candidate] = []
    for line in text.splitlines():
        header = re.match(r"## candidate q_tile=(\d+) k_tile=(\d+)", line)
        if header:
            current = {"q_tile": int(header.group(1)), "k_tile": int(header.group(2))}
            continue
        if not current:
            continue
        for key, cast in [
            ("causal_tile_pairs", int),
            ("median_s", float),
            ("effective_tflops", float),
            ("mps_encodes_per_s", float),
        ]:
            match = re.match(rf"{key}:\s*([-+0-9.eE]+)", line)
            if match:
                current[key] = cast(match.group(1))
        required = {
            "q_tile",
            "k_tile",
            "causal_tile_pairs",
            "median_s",
            "effective_tflops",
            "mps_encodes_per_s",
        }
        if required.issubset(current):
            candidates.append(
                Candidate(
                    q_tile=int(current["q_tile"]),
                    k_tile=int(current["k_tile"]),
                    pairs=int(current["causal_tile_pairs"]),
                    median_s=float(current["median_s"]),
                    tflops=float(current["effective_tflops"]),
                    encodes_per_s=float(current["mps_encodes_per_s"]),
                )
            )
            current = {}
    return candidates


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("grid", type=Path)
    parser.add_argument("--heads", type=int, default=8)
    parser.add_argument("--layers", type=int, default=6)
    args = parser.parse_args()
    candidates = parse(args.grid.read_text(encoding="utf-8"))
    print("tiled_attention_qk_mps_grid_analysis")
    print(f"grid: {args.grid}")
    print(f"candidate_count: {len(candidates)}")
    if not candidates:
        print("status: no_candidates")
        return
    best = min(candidates, key=lambda item: item.median_s)
    print(
        "best: "
        f"q_tile={best.q_tile} "
        f"k_tile={best.k_tile} "
        f"pairs={best.pairs} "
        f"median_s={best.median_s:.9f} "
        f"effective_tflops={best.tflops:.3f} "
        f"mps_encodes_per_s={best.encodes_per_s:.3f}"
    )
    print(f"projected_qk_heads_s: {best.median_s * args.heads:.9f}")
    print(f"projected_qk_layers_s: {best.median_s * args.heads * args.layers:.9f}")
    print("| q_tile | k_tile | pairs | median_s | TFLOPS | encodes_per_s |")
    print("|---:|---:|---:|---:|---:|---:|")
    for item in sorted(candidates, key=lambda candidate: candidate.median_s):
        print(
            f"| {item.q_tile} | {item.k_tile} | {item.pairs} | "
            f"{item.median_s:.9f} | {item.tflops:.3f} | {item.encodes_per_s:.3f} |"
        )


if __name__ == "__main__":
    main()
