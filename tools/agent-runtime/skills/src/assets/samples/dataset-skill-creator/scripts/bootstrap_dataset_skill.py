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


def auto_strategy(archetype: str, source_kind: str) -> str:
    if source_kind == "ticket-history":
        if archetype == "operating-model":
            return "ticket-operating-model"
        if archetype == "lookup-reference":
            return "ticket-dataset-knowledge"
    raise SystemExit(
        f"no automatic analysis strategy for archetype={archetype!r} source_kind={source_kind!r}; "
        "provide --analysis-strategy explicitly or add a matching analyzer"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Analyze a raw dataset and turn it into a generated skill.")
    parser.add_argument("--input", required=True)
    parser.add_argument("--source-kind", default="ticket-history")
    parser.add_argument("--skill-name", required=True)
    parser.add_argument("--skill-path", required=True)
    parser.add_argument("--archetype", required=True, choices=["operating-model", "lookup-reference", "workflow", "policy-gate"])
    parser.add_argument("--dataset-label", required=True)
    parser.add_argument("--goal", required=True)
    parser.add_argument("--analysis-dir", required=True)
    parser.add_argument("--analysis-strategy", choices=["auto", "ticket-operating-model", "ticket-dataset-knowledge"], default="auto")
    parser.add_argument("--display-name")
    parser.add_argument("--short-description")
    parser.add_argument("--default-prompt")
    parser.add_argument("--top-families", type=int, default=18)
    parser.add_argument("--min-family-size", type=int, default=25)
    parser.add_argument("--semantic-sample-size", type=int, default=120)
    parser.add_argument("--openai-model")
    parser.add_argument("--openai-refine-limit", type=int, default=0)
    parser.add_argument("--openai-base-url", default=os.getenv("OPENAI_BASE_URL", "https://api.openai.com"))
    parser.add_argument("--openai-api-key-env", default="OPENAI_API_KEY")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    input_path = Path(args.input)
    analysis_dir = Path(args.analysis_dir)
    strategy = auto_strategy(args.archetype, args.source_kind) if args.analysis_strategy == "auto" else args.analysis_strategy

    env = os.environ.copy()
    openai_key = env.get(args.openai_api_key_env)

    if strategy == "ticket-operating-model":
        command = [
            sys.executable,
            str(REPO_ROOT / "skills/system/ticket-operating-model-bootstrap/scripts/build_ticket_operating_model.py"),
            "--input",
            str(input_path),
            "--output-dir",
            str(analysis_dir),
            "--top-families",
            str(args.top_families),
            "--min-family-size",
            str(args.min_family_size),
        ]
        if args.openai_model:
            command.extend(
                [
                    "--openai-model",
                    args.openai_model,
                    "--openai-base-url",
                    args.openai_base_url,
                    "--openai-refine-limit",
                    str(args.openai_refine_limit),
                ]
            )
            if not openai_key:
                raise SystemExit(f"{args.openai_api_key_env} must be set when --openai-model is used")
        run(command, env=env)
        query_command = (
            f"python3 skills/system/ticket-operating-model-bootstrap/scripts/query_ticket_operating_model.py "
            f"--model-dir {analysis_dir} --query '<new ticket text>' --top-k 3"
        )
    elif strategy == "ticket-dataset-knowledge":
        command = [
            sys.executable,
            str(REPO_ROOT / "skills/system/ticket-dataset-knowledge-bootstrap/scripts/build_ticket_dataset_knowledgebase.py"),
            "--input-xlsx",
            str(input_path),
            "--output-dir",
            str(analysis_dir),
            "--semantic-sample-size",
            str(args.semantic_sample_size),
        ]
        run(command, env=env)
        query_command = None
    else:
        raise SystemExit(f"unsupported analysis strategy: {strategy}")

    create_command = [
        sys.executable,
        str(REPO_ROOT / "skills/system/dataset-skill-creator/scripts/create_dataset_skill.py"),
        "--skill-name",
        args.skill_name,
        "--skill-path",
        args.skill_path,
        "--archetype",
        args.archetype,
        "--dataset-label",
        args.dataset_label,
        "--goal",
        args.goal,
        "--analysis-dir",
        str(analysis_dir),
    ]
    if query_command:
        create_command.extend(["--query-command", query_command])
    if args.display_name:
        create_command.extend(["--display-name", args.display_name])
    if args.short_description:
        create_command.extend(["--short-description", args.short_description])
    if args.default_prompt:
        create_command.extend(["--default-prompt", args.default_prompt])
    run(create_command, env=env)

    print(
        json.dumps(
            {
                "input": str(input_path),
                "strategy": strategy,
                "analysis_dir": str(analysis_dir),
                "generated_skill_dir": str(Path(args.skill_path) / args.skill_name),
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
