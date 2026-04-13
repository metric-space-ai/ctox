#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile
import unittest


BOOTSTRAP_PATH = pathlib.Path(__file__).with_name("incident_bootstrap.py")
BOOTSTRAP_SPEC = importlib.util.spec_from_file_location("incident_bootstrap", BOOTSTRAP_PATH)
BOOTSTRAP_MODULE = importlib.util.module_from_spec(BOOTSTRAP_SPEC)
assert BOOTSTRAP_SPEC and BOOTSTRAP_SPEC.loader
BOOTSTRAP_SPEC.loader.exec_module(BOOTSTRAP_MODULE)

STORE_PATH = pathlib.Path(__file__).parents[2] / "discovery-graph" / "scripts" / "discovery_store.py"
STORE_SPEC = importlib.util.spec_from_file_location("discovery_store", STORE_PATH)
STORE_MODULE = importlib.util.module_from_spec(STORE_SPEC)
assert STORE_SPEC and STORE_SPEC.loader
STORE_SPEC.loader.exec_module(STORE_MODULE)


class IncidentBootstrapTests(unittest.TestCase):
    def test_build_graph_creates_incident_entities(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "ops.db")
            conn = STORE_MODULE.open_db(db_path)
            STORE_MODULE.store_capture(
                conn,
                {
                    "skill_key": "incident_response",
                    "run_id": "run-incident-001",
                    "collectors": [
                        {
                            "skill_key": "incident_response",
                            "run_id": "run-incident-001",
                            "collector": "incident_overview",
                            "captures": [
                                {
                                    "tool": "hostnamectl",
                                    "argv": ["hostnamectl"],
                                    "available": True,
                                    "stdout": "Static hostname: test-host\n",
                                    "stderr": "",
                                    "exit_code": 0,
                                    "started_at": "2026-03-25T00:00:00Z",
                                    "finished_at": "2026-03-25T00:00:01Z",
                                }
                            ],
                        },
                        {
                            "skill_key": "incident_response",
                            "run_id": "run-incident-001",
                            "collector": "incident_service",
                            "captures": [
                                {
                                    "tool": "systemctl",
                                    "argv": ["systemctl", "status", "demo.service", "--no-pager"],
                                    "available": True,
                                    "stdout": "demo.service - Demo\n   Active: failed (Result: exit-code)\n",
                                    "stderr": "",
                                    "exit_code": 3,
                                    "started_at": "2026-03-25T00:00:01Z",
                                    "finished_at": "2026-03-25T00:00:02Z",
                                }
                            ],
                        },
                    ],
                },
                "local",
            )
            graph = BOOTSTRAP_MODULE.build_graph(conn, "run-incident-001")
            kinds = {item["kind"] for item in graph["entities"]}
            relations = {item["relation"] for item in graph["relations"]}
            self.assertIn("incident_case", kinds)
            self.assertIn("hypothesis_set", kinds)
            self.assertIn("mitigation_action", kinds)
            self.assertIn("status_update", kinds)
            self.assertIn("affects", relations)
            self.assertIn("suggests", relations)


if __name__ == "__main__":
    unittest.main()
