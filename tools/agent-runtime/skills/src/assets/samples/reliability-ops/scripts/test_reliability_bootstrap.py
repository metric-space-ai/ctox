#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile
import unittest


BOOTSTRAP_PATH = pathlib.Path(__file__).with_name("reliability_bootstrap.py")
BOOTSTRAP_SPEC = importlib.util.spec_from_file_location("reliability_bootstrap", BOOTSTRAP_PATH)
BOOTSTRAP_MODULE = importlib.util.module_from_spec(BOOTSTRAP_SPEC)
assert BOOTSTRAP_SPEC and BOOTSTRAP_SPEC.loader
BOOTSTRAP_SPEC.loader.exec_module(BOOTSTRAP_MODULE)

STORE_PATH = pathlib.Path(__file__).parents[2] / "discovery-graph" / "scripts" / "discovery_store.py"
STORE_SPEC = importlib.util.spec_from_file_location("discovery_store", STORE_PATH)
STORE_MODULE = importlib.util.module_from_spec(STORE_SPEC)
assert STORE_SPEC and STORE_SPEC.loader
STORE_SPEC.loader.exec_module(STORE_MODULE)


class ReliabilityBootstrapTests(unittest.TestCase):
    def test_build_graph_creates_assessment_and_anomalies(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "ops.db")
            conn = STORE_MODULE.open_db(db_path)
            STORE_MODULE.store_capture(
                conn,
                {
                    "skill_key": "reliability_ops",
                    "run_id": "run-001",
                    "collectors": [
                        {
                            "skill_key": "reliability_ops",
                            "run_id": "run-001",
                            "collector": "cpu_memory",
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
                                },
                                {
                                    "tool": "free",
                                    "argv": ["free", "-m"],
                                    "available": True,
                                    "stdout": "              total        used        free      shared  buff/cache   available\nMem:           1000         920          20          10          60          40\nSwap:           512          20         492\n",
                                    "stderr": "",
                                    "exit_code": 0,
                                    "started_at": "2026-03-25T00:00:01Z",
                                    "finished_at": "2026-03-25T00:00:02Z",
                                },
                            ],
                        },
                        {
                            "skill_key": "reliability_ops",
                            "run_id": "run-001",
                            "collector": "service_status",
                            "captures": [
                                {
                                    "tool": "systemctl",
                                    "argv": ["systemctl", "--failed", "--no-pager", "--no-legend", "--plain"],
                                    "available": True,
                                    "stdout": "demo.service loaded failed failed Demo Service\n",
                                    "stderr": "",
                                    "exit_code": 0,
                                    "started_at": "2026-03-25T00:00:02Z",
                                    "finished_at": "2026-03-25T00:00:03Z",
                                }
                            ],
                        },
                        {
                            "skill_key": "reliability_ops",
                            "run_id": "run-001",
                            "collector": "service_logs",
                            "captures": [
                                {
                                    "tool": "journalctl",
                                    "argv": ["journalctl", "-p", "warning", "-n", "160", "--no-pager"],
                                    "available": True,
                                    "stdout": "Mar 25 host demo.service: failed to start dependency\n",
                                    "stderr": "",
                                    "exit_code": 0,
                                    "started_at": "2026-03-25T00:00:03Z",
                                    "finished_at": "2026-03-25T00:00:04Z",
                                }
                            ],
                        },
                    ],
                },
                "local",
            )
            graph = BOOTSTRAP_MODULE.build_graph(conn, "run-001")
            kinds = {item["kind"] for item in graph["entities"]}
            relations = {item["relation"] for item in graph["relations"]}
            self.assertIn("health_assessment", kinds)
            self.assertIn("resource_pressure", kinds)
            self.assertIn("anomaly", kinds)
            self.assertIn("remediation_suggestion", kinds)
            self.assertIn("assesses", relations)
            self.assertIn("affects", relations)
            self.assertIn("suggests", relations)


if __name__ == "__main__":
    unittest.main()
