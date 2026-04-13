#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[4]


def run(command: list[str], env: dict[str, str] | None = None) -> None:
    completed = subprocess.run(command, cwd=REPO_ROOT, env=env, check=False)
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Export live ticket history, build a desk-specific skill, evaluate it, and bind it to the source system."
    )
    parser.add_argument("--system", required=True)
    parser.add_argument("--skill-name", required=True)
    parser.add_argument("--dataset-label", required=True)
    parser.add_argument("--goal", required=True)
    parser.add_argument("--analysis-dir", required=True)
    parser.add_argument("--skill-path", default="runtime/generated-skills")
    parser.add_argument("--artifact-dir", default="runtime/inputs")
    parser.add_argument("--cases", type=int, default=5)
    parser.add_argument("--top-families", type=int, default=18)
    parser.add_argument("--min-family-size", type=int, default=3)
    parser.add_argument("--min-eval-cases", type=int, default=2)
    parser.add_argument("--min-top1-hit-rate", type=float, default=0.6)
    parser.add_argument("--min-decision-support-completeness", type=float, default=1.0)
    parser.add_argument("--openai-model")
    parser.add_argument("--openai-refine-limit", type=int, default=0)
    parser.add_argument("--insight-output-dir")
    parser.add_argument("--skip-insight-build", action="store_true")
    parser.add_argument("--ctox-bin", default="ctox")
    parser.add_argument("--ctox-root")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    ctox_env = os.environ.copy()
    if args.ctox_root:
        ctox_env["CTOX_ROOT"] = args.ctox_root
    artifact_dir = Path(args.artifact_dir)
    artifact_dir.mkdir(parents=True, exist_ok=True)
    export_path = artifact_dir / f"{args.system}_ticket_history.jsonl"

    run(
        [
            args.ctox_bin,
            "ticket",
            "history-export",
            "--system",
            args.system,
            "--output",
            str(export_path),
        ],
        env=ctox_env,
    )

    bootstrap_command = [
        sys.executable,
        str(REPO_ROOT / "skills/system/dataset-skill-creator/scripts/bootstrap_dataset_skill.py"),
        "--input",
        str(export_path),
        "--source-kind",
        "ticket-history",
        "--skill-name",
        args.skill_name,
        "--skill-path",
        args.skill_path,
        "--archetype",
        "operating-model",
        "--dataset-label",
        args.dataset_label,
        "--goal",
        args.goal,
        "--analysis-dir",
        args.analysis_dir,
        "--top-families",
        str(args.top_families),
        "--min-family-size",
        str(args.min_family_size),
    ]
    if args.openai_model:
        bootstrap_command.extend(
            [
                "--openai-model",
                args.openai_model,
                "--openai-refine-limit",
                str(args.openai_refine_limit),
            ]
        )
    run(bootstrap_command, env=os.environ.copy())

    skill_dir = Path(args.skill_path) / args.skill_name
    eval_output = Path(args.analysis_dir) / "generated_skill_eval.md"
    run(
        [
            sys.executable,
            str(REPO_ROOT / "skills/system/dataset-skill-creator/scripts/bench_generated_skill.py"),
            "--skill-dir",
            str(skill_dir),
            "--model-dir",
            str(args.analysis_dir),
            "--input",
            str(export_path),
            "--cases",
            str(args.cases),
            "--min-family-size",
            str(args.min_family_size),
            "--output",
            str(eval_output),
        ],
        env=os.environ.copy(),
    )

    eval_json = json.loads(eval_output.with_suffix(".json").read_text(encoding="utf-8"))
    summary = eval_json["summary"]
    case_count = int(summary.get("case_count", 0))
    if case_count < args.min_eval_cases:
        raise SystemExit(
            f"generated skill evaluation only produced {case_count} usable cases; "
            f"need at least {args.min_eval_cases} before activating the source skill"
        )
    top1_hit_rate = float(summary.get("top1_hit_rate", 0.0))
    if top1_hit_rate < args.min_top1_hit_rate:
        raise SystemExit(
            f"generated skill top-1 family hit rate {top1_hit_rate:.2f} is below the activation threshold "
            f"{args.min_top1_hit_rate:.2f}"
        )
    decision_support_completeness = float(summary.get("decision_support_completeness", 0.0))
    if decision_support_completeness < args.min_decision_support_completeness:
        raise SystemExit(
            f"generated skill decision-support completeness {decision_support_completeness:.2f} is below the activation threshold "
            f"{args.min_decision_support_completeness:.2f}"
        )
    if not summary.get("all_required_sections_present", False):
        raise SystemExit("generated skill is missing required operator sections; refusing to activate source skill")
    if not summary.get("content_leak_free", False):
        raise SystemExit("generated skill still leaks tooling/internal content; refusing to activate source skill")
    if not summary.get("language_review_clean", False):
        raise SystemExit("generated skill failed language review; refusing to activate source skill")
    if int(summary.get("generic_family_case_count", 0)) > 0:
        raise SystemExit("generated skill still relies on generic family buckets; refusing to activate source skill")

    run(
        [
            args.ctox_bin,
            "ticket",
            "source-skill-set",
            "--system",
            args.system,
            "--skill",
            args.skill_name,
            "--archetype",
            "operating-model",
            "--status",
            "active",
            "--origin",
            "ticket-onboarding",
            "--artifact-path",
            str(skill_dir),
            "--notes",
            f"Generated from mirrored {args.system} ticket history during onboarding.",
        ],
        env=ctox_env,
    )

    guide_script = REPO_ROOT / "skills/system/ticket-system-onboarding/scripts/upsert_onboarding_guide.py"
    if guide_script.exists():
        run(
            [
                sys.executable,
                str(guide_script),
                "--ctox-bin",
                args.ctox_bin,
                "--system",
                args.system,
                "--assign-self",
            ],
            env=ctox_env,
        )

    print(
        json.dumps(
            {
                "system": args.system,
                "export_path": str(export_path),
                "analysis_dir": args.analysis_dir,
                "skill_dir": str(skill_dir),
                "evaluation_report": str(eval_output),
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
