#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile
import unittest


BOOTSTRAP_PATH = pathlib.Path(__file__).with_name("security_bootstrap.py")
BOOTSTRAP_SPEC = importlib.util.spec_from_file_location("security_bootstrap", BOOTSTRAP_PATH)
BOOTSTRAP_MODULE = importlib.util.module_from_spec(BOOTSTRAP_SPEC)
assert BOOTSTRAP_SPEC and BOOTSTRAP_SPEC.loader
BOOTSTRAP_SPEC.loader.exec_module(BOOTSTRAP_MODULE)

STORE_PATH = pathlib.Path(__file__).parents[2] / "discovery-graph" / "scripts" / "discovery_store.py"
STORE_SPEC = importlib.util.spec_from_file_location("discovery_store", STORE_PATH)
STORE_MODULE = importlib.util.module_from_spec(STORE_SPEC)
assert STORE_SPEC and STORE_SPEC.loader
STORE_SPEC.loader.exec_module(STORE_MODULE)


class SecurityBootstrapTests(unittest.TestCase):
    def test_build_graph_creates_snapshot_and_findings(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "ops.db")
            conn = STORE_MODULE.open_db(db_path)
            STORE_MODULE.store_capture(
                conn,
                {
                    "skill_key": "security_posture",
                    "run_id": "run-security-001",
                    "collectors": [
                        {
                            "skill_key": "security_posture",
                            "run_id": "run-security-001",
                            "collector": "listeners",
                            "captures": [
                                {
                                    "tool": "ss",
                                    "argv": ["ss", "-tulpnH"],
                                    "available": True,
                                    "stdout": "tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:*\n",
                                    "stderr": "",
                                    "exit_code": 0,
                                    "started_at": "2026-03-25T00:00:00Z",
                                    "finished_at": "2026-03-25T00:00:01Z",
                                }
                            ],
                        }
                    ],
                },
                "local",
            )
            graph = BOOTSTRAP_MODULE.build_graph(conn, "run-security-001")
            kinds = {item["kind"] for item in graph["entities"]}
            self.assertIn("compliance_snapshot", kinds)
            self.assertIn("security_finding", kinds)
            self.assertIn("remediation_plan", kinds)


if __name__ == "__main__":
    unittest.main()
