#!/usr/bin/env python3
import tempfile
from pathlib import Path

from record_skill_review import append_review


def test_append_review_creates_structured_entry() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        ledger = Path(tmp) / "skill-improvement-ledger.md"
        append_review(
            ledger,
            "successful",
            "Improved mail context lookup",
            "Make replies consider earlier communication",
            "channel history and search returned relevant context",
            "owner-communication,communication-context",
        )
        text = ledger.read_text(encoding="utf-8")
        assert "# Skill Improvement Ledger" in text
        assert "- Status: successful" in text
        assert "- Change: Improved mail context lookup" in text


if __name__ == "__main__":
    test_append_review_creates_structured_entry()
    print("ok")
