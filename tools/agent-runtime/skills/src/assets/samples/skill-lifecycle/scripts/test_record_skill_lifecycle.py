#!/usr/bin/env python3
import tempfile
import unittest
from pathlib import Path

import record_skill_lifecycle


class RecordSkillLifecycleTests(unittest.TestCase):
    def test_appends_transition(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            ledger = Path(temp_dir) / "skill-lifecycle-ledger.md"
            record_skill_lifecycle.append_transition(
                ledger=ledger,
                skill="service-deployment",
                from_state="candidate",
                to_state="promoted",
                reason="generic acceptance gate hardened",
                evidence="verify_contract tests green",
            )
            content = ledger.read_text(encoding="utf-8")
            self.assertIn("# Skill Lifecycle Ledger", content)
            self.assertIn("- Skill: service-deployment", content)
            self.assertIn("- From: candidate", content)
            self.assertIn("- To: promoted", content)


if __name__ == "__main__":
    unittest.main()
