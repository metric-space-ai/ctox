#!/usr/bin/env python3
import argparse
import json
import subprocess
import sys
from pathlib import Path


def run_json(command: list[str]) -> dict:
    result = subprocess.run(command, capture_output=True, text=True, check=True)
    return json.loads(result.stdout)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--generated-dir", required=True)
    parser.add_argument("--email-file", required=True)
    parser.add_argument("--subject", required=True)
    parser.add_argument(
        "--send-policy",
        choices=["suggestion", "draft", "send"],
        default="suggestion",
    )
    args = parser.parse_args()

    generated_dir = Path(args.generated_dir)
    skill_root = Path(__file__).resolve().parents[1]
    resolver = skill_root / "scripts" / "resolve_runbook_item.py"
    composer = skill_root / "scripts" / "compose_support_reply.py"

    resolution = run_json(
        [
            sys.executable,
            str(resolver),
            "--items",
            str(generated_dir / "runbook_items.jsonl"),
            "--email-file",
            args.email_file,
        ]
    )

    if resolution["decision"] != "matched" or not resolution.get("best_match"):
        print(
            json.dumps(
                {
                    "decision": "needs_review",
                    "resolution": resolution,
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return

    reply = run_json(
        [
            sys.executable,
            str(composer),
            "--main-skill",
            str(generated_dir / "main_skill.json"),
            "--skillbook",
            str(generated_dir / "skillbook.json"),
            "--items",
            str(generated_dir / "runbook_items.jsonl"),
            "--item-id",
            resolution["best_match"]["item_id"],
            "--email-file",
            args.email_file,
            "--subject",
            args.subject,
            "--send-policy",
            args.send_policy,
        ]
    )

    print(
        json.dumps(
            {
                "decision": reply["decision"],
                "resolution": resolution,
                "reply": reply,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
