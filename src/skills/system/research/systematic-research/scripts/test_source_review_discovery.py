#!/usr/bin/env python3
"""Regression tests for source-review discovery safety boundaries."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("source_review_discovery.py")


class SourceReviewDiscoveryTests(unittest.TestCase):
    def test_ctox_deep_research_is_the_default_backend(self) -> None:
        source = SCRIPT.read_text(encoding="utf-8")
        self.assertIn('default="ctox-deep-research"', source)
        self.assertNotIn('default="hybrid"', source)

    def test_business_writeback_fails_closed_without_invoking_psql(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            marker = root / "psql-invoked"
            fake_psql = root / "psql"
            fake_psql.write_text(f"#!/bin/sh\ntouch {marker}\n", encoding="utf-8")
            fake_psql.chmod(0o755)
            out_dir = root / "out"

            env = os.environ.copy()
            env["PATH"] = f"{root}{os.pathsep}{env.get('PATH', '')}"
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--topic",
                    "example topic",
                    "--out-dir",
                    str(out_dir),
                    "--allow-auto-query-plan",
                    "--business-writeback",
                    "--business-database-url",
                    "postgresql://unsafe.example/research",
                    "--business-research-run-id",
                    "run-1",
                ],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("--business-writeback is retired and disabled", result.stderr)
            self.assertIn("never writes PostgreSQL or Business OS state", result.stderr)
            self.assertFalse(marker.exists(), "fail-closed option must not invoke psql")
            self.assertFalse(out_dir.exists(), "fail-closed option must not start discovery")

    def test_plan_only_discovery_still_writes_query_plan(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            queries = root / "queries.csv"
            queries.write_text("focus,query\nseed,example topic sources\n", encoding="utf-8")
            out_dir = root / "out"
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--topic",
                    "example topic",
                    "--out-dir",
                    str(out_dir),
                    "--queries-file",
                    str(queries),
                    "--plan-only",
                ],
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                (out_dir / "query_plan.csv").read_text(encoding="utf-8"),
                "focus,query\nseed,example topic sources\n",
            )


if __name__ == "__main__":
    unittest.main()
