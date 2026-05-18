#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile
import unittest


QUERY_PATH = pathlib.Path(__file__).with_name("discovery_query.py")
QUERY_SPEC = importlib.util.spec_from_file_location("discovery_query", QUERY_PATH)
QUERY_MODULE = importlib.util.module_from_spec(QUERY_SPEC)
assert QUERY_SPEC and QUERY_SPEC.loader
QUERY_SPEC.loader.exec_module(QUERY_MODULE)

STORE_PATH = pathlib.Path(__file__).with_name("discovery_store.py")
STORE_SPEC = importlib.util.spec_from_file_location("discovery_store", STORE_PATH)
STORE_MODULE = importlib.util.module_from_spec(STORE_SPEC)
assert STORE_SPEC and STORE_SPEC.loader
STORE_SPEC.loader.exec_module(STORE_MODULE)


class DiscoveryQueryTests(unittest.TestCase):
    def test_summary_reports_counts_and_gaps(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = STORE_MODULE.open_db(db_path)
            STORE_MODULE.store_capture(
                conn,
                {
                    "collector": "containers",
                    "captures": [
                        {
                            "tool": "docker",
                            "argv": ["docker", "ps"],
                            "available": False,
                            "stdout": "",
                            "stderr": "command not found: docker",
                            "exit_code": None,
                            "finished_at": "2026-03-25T00:00:01Z",
                        }
                    ],
                },
                "local",
            )
            STORE_MODULE.store_graph(
                conn,
                {
                    "run_id": "run-001",
                    "status": "normalized",
                    "entities": [
                        {
                            "kind": "host",
                            "natural_key": "host:a",
                            "title": "a",
                            "attrs": {},
                        }
                    ],
                    "relations": [],
                    "evidence": [],
                },
            )
            payload = QUERY_MODULE.summary(conn)
            self.assertEqual(payload["runs"][0]["status"], "normalized")
            host_row = next(item for item in payload["entities"] if item["kind"] == "host")
            self.assertEqual(host_row["active"], 1)
            self.assertEqual(payload["active_coverage_gaps"][0]["natural_key"], "coverage_gap:local:containers:docker")

    def test_export_cytoscape_returns_nodes_and_edges(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = STORE_MODULE.open_db(db_path)
            STORE_MODULE.store_graph(
                conn,
                {
                    "run_id": "run-001",
                    "status": "normalized",
                    "entities": [
                        {
                            "kind": "host",
                            "natural_key": "host:a",
                            "title": "a",
                            "attrs": {},
                        },
                        {
                            "kind": "systemd_unit",
                            "natural_key": "systemd_unit:ssh.service",
                            "title": "ssh.service",
                            "attrs": {},
                        },
                    ],
                    "relations": [
                        {
                            "from": {
                                "kind": "systemd_unit",
                                "natural_key": "systemd_unit:ssh.service",
                            },
                            "relation": "runs_on",
                            "to": {
                                "kind": "host",
                                "natural_key": "host:a",
                            },
                            "attrs": {},
                        }
                    ],
                    "evidence": [],
                },
            )
            payload = QUERY_MODULE.export_cytoscape(conn)
            self.assertEqual(len(payload["nodes"]), 2)
            self.assertEqual(len(payload["edges"]), 1)
            self.assertEqual(payload["edges"][0]["data"]["relation"], "runs_on")

    def test_summary_can_filter_by_skill_key(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = STORE_MODULE.open_db(db_path)
            STORE_MODULE.store_graph(
                conn,
                {
                    "run_id": "run-discovery",
                    "skill_key": "discovery_graph",
                    "status": "normalized",
                    "entities": [
                        {"kind": "host", "natural_key": "host:a", "title": "a", "attrs": {}}
                    ],
                    "relations": [],
                    "evidence": [],
                },
            )
            STORE_MODULE.store_graph(
                conn,
                {
                    "run_id": "run-reliability",
                    "skill_key": "reliability_ops",
                    "status": "normalized",
                    "entities": [
                        {
                            "kind": "health_assessment",
                            "natural_key": "health_assessment:a",
                            "title": "ha",
                            "attrs": {},
                        }
                    ],
                    "relations": [],
                    "evidence": [],
                },
            )
            payload = QUERY_MODULE.summary(conn, "reliability_ops")
            self.assertEqual(len(payload["runs"]), 1)
            self.assertEqual(payload["runs"][0]["skill_key"], "reliability_ops")
            self.assertEqual(payload["entities"][0]["kind"], "health_assessment")


if __name__ == "__main__":
    unittest.main()
