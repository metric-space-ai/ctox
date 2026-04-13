#!/usr/bin/env python3
import argparse
from pathlib import Path


def append_review(
    ledger: Path,
    status: str,
    summary: str,
    goal: str,
    evidence: str,
    skills: str,
) -> None:
    ledger.parent.mkdir(parents=True, exist_ok=True)
    if not ledger.exists():
        ledger.write_text("# Skill Improvement Ledger\n\n", encoding="utf-8")
    entry = "\n".join(
        [
            "## Review Entry",
            f"- Status: {status.strip()}",
            f"- Skills: {skills.strip()}",
            f"- Goal: {goal.strip()}",
            f"- Change: {summary.strip()}",
            f"- Evidence: {evidence.strip()}",
            "",
        ]
    )
    with ledger.open("a", encoding="utf-8") as handle:
        handle.write(entry)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger", required=True)
    parser.add_argument("--status", required=True)
    parser.add_argument("--summary", required=True)
    parser.add_argument("--goal", required=True)
    parser.add_argument("--evidence", required=True)
    parser.add_argument("--skills", required=True)
    args = parser.parse_args()

    append_review(
        Path(args.ledger),
        args.status,
        args.summary,
        args.goal,
        args.evidence,
        args.skills,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
