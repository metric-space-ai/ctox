#!/usr/bin/env python3
"""Negative provenance and owned-process cleanup checks; no browser launches."""
import hashlib
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("diagnostic", Path(__file__).with_name("command-performance-diagnostic.py"))
diagnostic = importlib.util.module_from_spec(spec)
spec.loader.exec_module(diagnostic)


class DiagnosticTests(unittest.TestCase):
    def test_wrong_source_or_binary_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid",
                            "commit", "-q", "--allow-empty", "-m", "fixture"], cwd=root, check=True)
            source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
            binary = root / "binary"
            binary.write_bytes(b"fixture binary")
            digest = hashlib.sha256(binary.read_bytes()).hexdigest()
            proof = root / "proof"
            proof.mkdir()
            (proof / "source-revision.txt").write_text(source)
            (proof / "binary.sha256").write_text(digest + "  ctox\n")
            (proof / "build-profile.txt").write_text("profile=dev\n")
            # These fixture files are excluded, as downloaded artifacts are in
            # separate directories in CI; tracked source must stay clean.
            (root / ".git/info/exclude").write_text("binary\nproof/\n")
            self.assertEqual(diagnostic.provenance(root, binary, proof, source, digest)["source"], source)
            with self.assertRaisesRegex(ValueError, "source identity"):
                diagnostic.provenance(root, binary, proof, "0" * 40, digest)
            binary.write_bytes(b"different binary")
            with self.assertRaisesRegex(ValueError, "checksum"):
                diagnostic.provenance(root, binary, proof, source, digest)

    def test_deadline_closes_owned_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pid_file = root / "pid"
            code = "import os,time,pathlib;pathlib.Path('pid').write_text(str(os.getpid()));time.sleep(60)"
            with self.assertRaisesRegex(RuntimeError, "deadline"):
                diagnostic.bounded_run([sys.executable, "-c", code], root, os.environ.copy(),
                                       root / "log", timeout=0.5)
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid_file.read_text()), 0)

    def test_log_budget_stops_owned_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            code = "import time;print('x'*4096,flush=True);time.sleep(60)"
            with self.assertRaisesRegex(RuntimeError, "log-byte-limit"):
                diagnostic.bounded_run([sys.executable, "-c", code], root, os.environ.copy(),
                                       root / "log", timeout=5, byte_limit=1024)


if __name__ == "__main__":
    unittest.main()
