#!/usr/bin/env python3
"""Rule-out probe for a Qwen3.5-0.8B Core ML / ANE baseline.

The Metal decode path is hand-written and stateful. A fair ANE comparison needs
an existing Core ML artifact or a local converter path. This probe is deliberately
read-only: it records whether such a path is present before the research gate is
marked as measured or ruled out.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path


COREML_SUFFIXES = {".mlmodel", ".mlpackage", ".mlmodelc"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Probe repository root.",
    )
    parser.add_argument(
        "--model",
        type=Path,
        default=Path("/Users/michaelwelsch/.cache/huggingface/local/Qwen3.5-0.8B"),
        help="Local HF model directory.",
    )
    return parser.parse_args()


def find_coreml_artifacts(root: Path) -> list[str]:
    if not root.exists():
        return []
    artifacts: list[str] = []
    for path in root.rglob("*"):
        if path.suffix.lower() in COREML_SUFFIXES:
            artifacts.append(str(path))
    return sorted(artifacts)


def main() -> None:
    args = parse_args()
    repo_artifacts = find_coreml_artifacts(args.repo)
    model_artifacts = find_coreml_artifacts(args.model)
    coremltools_available = importlib.util.find_spec("coremltools") is not None

    status = "measurable" if repo_artifacts or model_artifacts else "ruled_out"
    reasons = []
    if not repo_artifacts and not model_artifacts:
        reasons.append("no .mlmodel/.mlpackage/.mlmodelc artifact found in repo or model dir")
    if not coremltools_available:
        reasons.append("coremltools is not installed in the current Python environment")
    reasons.append(
        "Qwen3.5 decode uses stateful DeltaNet/gated_delta_update and KV cache; no local Core ML converter path exists in this crate"
    )

    print(
        json.dumps(
            {
                "status": status,
                "repo": str(args.repo),
                "model": str(args.model),
                "coremltools_available": coremltools_available,
                "repo_coreml_artifacts": repo_artifacts,
                "model_coreml_artifacts": model_artifacts,
                "reasons": reasons,
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
