#!/usr/bin/env python3
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parent / "ctox_sudo.py"


class CtoxSudoTest(unittest.TestCase):
    def test_fails_cleanly_without_secret(self):
        with tempfile.TemporaryDirectory() as tmp:
            proc = subprocess.run(
                ["python3", str(SCRIPT), "--root", tmp, "true"],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("missing sudo secret reference", proc.stderr + proc.stdout)


if __name__ == "__main__":
    unittest.main()
