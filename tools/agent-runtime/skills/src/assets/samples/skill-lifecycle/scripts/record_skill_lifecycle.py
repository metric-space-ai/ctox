#!/usr/bin/env python3
import argparse
from pathlib import Path


def append_transition(
    ledger: Path,
    skill: str,
    from_state: str,
    to_state: str,
    reason: str,
    evidence: str,
) -> None:
    ledger.parent.mkdir(parents=True, exist_ok=True)
    if not ledger.exists():
        ledger.write_text("# Skill Lifecycle Ledger\n\n", encoding="utf-8")
    entry = "\n".join(
        [
            "## Skill Transition",
            f"- Skill: {skill.strip()}",
            f"- From: {from_state.strip()}",
            f"- To: {to_state.strip()}",
            f"- Reason: {reason.strip()}",
            f"- Evidence: {evidence.strip()}",
            "",
        ]
    )
    with ledger.open("a", encoding="utf-8") as handle:
        handle.write(entry)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ledger", required=True)
    parser.add_argument("--skill", required=True)
    parser.add_argument("--from-state", required=True)
    parser.add_argument("--to-state", required=True)
    parser.add_argument("--reason", required=True)
    parser.add_argument("--evidence", required=True)
    args = parser.parse_args()

    append_transition(
        Path(args.ledger),
        args.skill,
        args.from_state,
        args.to_state,
        args.reason,
        args.evidence,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
