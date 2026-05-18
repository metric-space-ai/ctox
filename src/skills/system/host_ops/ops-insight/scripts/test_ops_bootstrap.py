#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile
import unittest


BOOTSTRAP_PATH = pathlib.Path(__file__).with_name("ops_bootstrap.py")
BOOTSTRAP_SPEC = importlib.util.spec_from_file_location("ops_bootstrap", BOOTSTRAP_PATH)
BOOTSTRAP_MODULE = importlib.util.module_from_spec(BOOTSTRAP_SPEC)
assert BOOTSTRAP_SPEC and BOOTSTRAP_SPEC.loader
BOOTSTRAP_SPEC.loader.exec_module(BOOTSTRAP_MODULE)

STORE_PATH = pathlib.Path(__file__).parents[2] / "discovery-graph" / "scripts" / "discovery_store.py"
STORE_SPEC = importlib.util.spec_from_file_location("discovery_store", STORE_PATH)
STORE_MODULE = importlib.util.module_from_spec(STORE_SPEC)
assert STORE_SPEC and STORE_SPEC.loader
STORE_SPEC.loader.exec_module(STORE_MODULE)


class OpsBootstrapTests(unittest.TestCase):
    def test_build_graph_creates_report_entities(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "ops.db")
            conn = STORE_MODULE.open_db(db_path)
            STORE_MODULE.store_capture(
                conn,
                {
                    "skill_key": "ops_insight",
                    "run_id": "run-ops-001",
                    "collectors": [
                        {
                            "skill_key": "ops_insight",
                            "run_id": "run-ops-001",
                            "collector": "host_brief",
                            "captures": [
                                {
                                    "tool": "uptime",
                                    "argv": ["uptime"],
                                    "available": True,
                                    "stdout": " 10:00:00 up 10 days, 1 user, load average: 0.10, 0.20, 0.30\n",
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
            graph = BOOTSTRAP_MODULE.build_graph(conn, "run-ops-001")
            kinds = {item["kind"] for item in graph["entities"]}
            self.assertIn("scorecard", kinds)
            self.assertIn("decision_brief", kinds)
            self.assertIn("priority_backlog", kinds)
            self.assertIn("dashboard_view", kinds)


if __name__ == "__main__":
    unittest.main()
