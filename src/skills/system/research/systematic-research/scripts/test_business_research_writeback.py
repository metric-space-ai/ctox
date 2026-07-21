#!/usr/bin/env python3
"""Regression tests for the retired PostgreSQL research writeback bridge."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("business_research_writeback.py")
REFERENCE = SCRIPT.parent.parent / "references" / "decision-report-mode-full.md"


class RetiredWritebackTests(unittest.TestCase):
    def test_legacy_cli_fails_before_invoking_psql_or_writing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            marker = root / "psql-invoked"
            fake_psql = root / "psql"
            fake_psql.write_text(f"#!/bin/sh\ntouch {marker}\n", encoding="utf-8")
            fake_psql.chmod(0o755)
            payload = root / "payload.json"
            payload.write_text('{"sources": [], "graph": {"nodes": [], "edges": []}}', encoding="utf-8")

            env = os.environ.copy()
            env["PATH"] = f"{root}{os.pathsep}{env.get('PATH', '')}"
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--database-url",
                    "postgresql://unsafe.example/research",
                    "--store-key",
                    "marketing/research/runs",
                    "--run-id",
                    "run-1",
                    "--payload-json",
                    str(payload),
                ],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("retired", result.stderr)
            self.assertIn("native Business OS command path", result.stderr)
            self.assertFalse(marker.exists(), "retired entry point must not invoke psql")

    def test_script_contains_no_legacy_bridge_or_writeback_api(self) -> None:
        source = SCRIPT.read_text(encoding="utf-8")
        for forbidden in ("psql", "subprocess", "business_runtime_stores", "DATABASE_URL", "tempfile"):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, source)

    def test_full_mode_runbook_does_not_document_direct_writeback(self) -> None:
        document = REFERENCE.read_text(encoding="utf-8")
        self.assertIn("The former\n`business_research_writeback.py` PostgreSQL bridge is retired", document)
        self.assertIn("ctx.commandBus.dispatch", document)
        self.assertIn("research.systematic.run", document)
        self.assertIn("source_id", document)
        self.assertIn("snapshot_id", document)
        self.assertIn("evidence_id", document)
        self.assertNotIn("--database-url \"$DATABASE_URL\"", document)
        self.assertNotIn("--payload-json /tmp/BUSINESS_RESEARCH_RUN_ID_agent_curated_payload.json", document)


if __name__ == "__main__":
    unittest.main()
