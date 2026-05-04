#!/usr/bin/env python3
"""Validate an approximate/quantized kernel measurement against explicit gates."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


METRIC_RE = re.compile(r"^\s*([A-Za-z0-9_.-]+)\s*:\s*([-+0-9.eE]+)\s*$")


def load_metrics(path: Path) -> dict[str, float]:
    metrics: dict[str, float] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        match = METRIC_RE.match(line)
        if match:
            metrics[match.group(1)] = float(match.group(2))
    return metrics


def require_metric(metrics: dict[str, float], key: str) -> float:
    if key not in metrics:
        raise SystemExit(f"missing metric `{key}`")
    return metrics[key]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("measurement", type=Path)
    parser.add_argument("--max-abs-key", default="max_abs_error")
    parser.add_argument("--mean-abs-key", default="mean_abs_error")
    parser.add_argument("--max-abs", type=float, required=True)
    parser.add_argument("--mean-abs", type=float, required=True)
    parser.add_argument("--baseline-key", default="baseline_median_s")
    parser.add_argument("--candidate-key", default="candidate_median_s")
    parser.add_argument("--speedup-min", type=float, default=1.0)
    args = parser.parse_args()

    metrics = load_metrics(args.measurement)
    max_abs = require_metric(metrics, args.max_abs_key)
    mean_abs = require_metric(metrics, args.mean_abs_key)

    failures: list[str] = []
    if max_abs > args.max_abs:
        failures.append(f"{args.max_abs_key}={max_abs:.9g} > {args.max_abs:.9g}")
    if mean_abs > args.mean_abs:
        failures.append(f"{args.mean_abs_key}={mean_abs:.9g} > {args.mean_abs:.9g}")

    speedup = None
    if args.baseline_key in metrics and args.candidate_key in metrics:
        baseline = metrics[args.baseline_key]
        candidate = metrics[args.candidate_key]
        if candidate <= 0.0:
            failures.append(f"{args.candidate_key} must be > 0, got {candidate:.9g}")
        else:
            speedup = baseline / candidate
            if speedup < args.speedup_min:
                failures.append(f"speedup={speedup:.4f} < {args.speedup_min:.4f}")

    print("quant_error_gate")
    print(f"measurement: {args.measurement}")
    print(f"{args.max_abs_key}: {max_abs:.9g} limit={args.max_abs:.9g}")
    print(f"{args.mean_abs_key}: {mean_abs:.9g} limit={args.mean_abs:.9g}")
    if speedup is not None:
        print(f"speedup: {speedup:.4f} limit={args.speedup_min:.4f}")
    else:
        print(
            "speedup: unavailable "
            f"(missing `{args.baseline_key}` or `{args.candidate_key}`)"
        )

    if failures:
        print("validation: FAIL")
        for failure in failures:
            print(f"failure: {failure}")
        return 1

    print("validation: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
