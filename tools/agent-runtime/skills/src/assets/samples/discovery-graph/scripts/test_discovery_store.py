#!/usr/bin/env python3
import importlib.util
import pathlib
import sqlite3
import tempfile
import unittest


MODULE_PATH = pathlib.Path(__file__).with_name("discovery_store.py")
SPEC = importlib.util.spec_from_file_location("discovery_store", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class DiscoveryStoreTests(unittest.TestCase):
    def test_init_creates_schema(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = MODULE.open_db(db_path)
            tables = {
                row[0]
                for row in conn.execute(
                    "SELECT name FROM sqlite_master WHERE type='table'"
                ).fetchall()
            }
            self.assertIn("discovery_run", tables)
            self.assertIn("discovery_capture", tables)
            self.assertIn("discovery_entity", tables)
            self.assertIn("discovery_relation", tables)
            self.assertIn("discovery_evidence", tables)

    def test_store_capture_and_graph(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = MODULE.open_db(db_path)
            capture_result = MODULE.store_capture(
                conn,
                {
                    "collector": "host_identity",
                    "captured_at": "2026-03-25T00:00:00Z",
                    "captures": [
                        {
                            "tool": "hostnamectl",
                            "argv": ["hostnamectl"],
                            "stdout": "Static hostname: test-host\n",
                            "stderr": "",
                            "exit_code": 0,
                            "finished_at": "2026-03-25T00:00:01Z",
                        }
                    ],
                },
                "local",
            )
            capture_id = capture_result["capture_ids"][0]
            graph_result = MODULE.store_graph(
                conn,
                {
                    "run_id": capture_result["run_id"],
                    "entities": [
                        {
                            "kind": "host",
                            "natural_key": "host:test-host",
                            "title": "test-host",
                            "attrs": {"hostname": "test-host"},
                        }
                    ],
                    "relations": [],
                    "evidence": [
                        {
                            "capture_id": capture_id,
                            "entity": {
                                "kind": "host",
                                "natural_key": "host:test-host",
                            },
                            "note": "Derived from hostnamectl output.",
                        }
                    ],
                },
            )
            self.assertEqual(graph_result["entity_count"], 1)
            host_row = conn.execute(
                "SELECT natural_key, title FROM discovery_entity"
            ).fetchone()
            self.assertEqual(host_row, ("host:test-host", "test-host"))
            evidence_count = conn.execute(
                "SELECT COUNT(*) FROM discovery_evidence"
            ).fetchone()[0]
            self.assertEqual(evidence_count, 1)
            skill_key = conn.execute(
                "SELECT skill_key FROM discovery_run WHERE run_id = ?",
                (capture_result["run_id"],),
            ).fetchone()[0]
            self.assertEqual(skill_key, "discovery_graph")

    def test_store_capture_accepts_run_all_payload(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = MODULE.open_db(db_path)
            run_id = "run-shared-001"
            result = MODULE.store_capture(
                conn,
                {
                    "run_id": run_id,
                    "collectors": [
                        {
                            "run_id": run_id,
                            "collector": "host_identity",
                            "captured_at": "2026-03-25T00:00:00Z",
                            "captures": [
                                {
                                    "tool": "hostnamectl",
                                    "argv": ["hostnamectl"],
                                    "started_at": "2026-03-25T00:00:00Z",
                                    "stdout": "Static hostname: test-host\n",
                                    "stderr": "",
                                    "exit_code": 0,
                                    "finished_at": "2026-03-25T00:00:01Z",
                                }
                            ],
                        },
                        {
                            "run_id": run_id,
                            "collector": "services",
                            "captured_at": "2026-03-25T00:00:02Z",
                            "captures": [
                                {
                                    "tool": "systemctl",
                                    "argv": ["systemctl", "list-units"],
                                    "started_at": "2026-03-25T00:00:02Z",
                                    "stdout": "ssh.service loaded active running OpenBSD Secure Shell server\n",
                                    "stderr": "",
                                    "exit_code": 0,
                                    "finished_at": "2026-03-25T00:00:03Z",
                                }
                            ],
                        },
                    ]
                },
                "local",
            )
            self.assertEqual(len(result["capture_ids"]), 2)
            self.assertEqual(result["run_id"], run_id)
            count = conn.execute("SELECT COUNT(*) FROM discovery_capture").fetchone()[0]
            self.assertEqual(count, 2)
            run_count = conn.execute("SELECT COUNT(*) FROM discovery_run").fetchone()[0]
            self.assertEqual(run_count, 1)
            run_row = conn.execute(
                "SELECT status, started_at, finished_at FROM discovery_run WHERE run_id = ?",
                (run_id,),
            ).fetchone()
            self.assertEqual(run_row[0], "captured")
            self.assertEqual(run_row[1], "2026-03-25T00:00:00Z")
            self.assertEqual(run_row[2], "2026-03-25T00:00:03Z")

    def test_store_graph_marks_run_normalized(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = MODULE.open_db(db_path)
            MODULE.ensure_run(conn, "run-001")
            MODULE.store_graph(
                conn,
                {
                    "run_id": "run-001",
                    "status": "normalized",
                    "note": "stored graph facts",
                    "entities": [],
                    "relations": [],
                    "evidence": [],
                },
            )
            row = conn.execute(
                "SELECT status, note, finished_at FROM discovery_run WHERE run_id = 'run-001'"
            ).fetchone()
            self.assertEqual(row[0], "normalized")
            self.assertEqual(row[1], "stored graph facts")
            self.assertTrue(row[2])

    def test_full_sweep_marks_missing_rows_inactive(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = MODULE.open_db(db_path)
            MODULE.store_graph(
                conn,
                {
                    "run_id": "run-001",
                    "status": "normalized",
                    "full_sweep": True,
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
            MODULE.store_graph(
                conn,
                {
                    "run_id": "run-002",
                    "status": "normalized",
                    "full_sweep": True,
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
            host_active = conn.execute(
                "SELECT is_active FROM discovery_entity WHERE natural_key = 'host:a'"
            ).fetchone()[0]
            unit_active = conn.execute(
                "SELECT is_active FROM discovery_entity WHERE natural_key = 'systemd_unit:ssh.service'"
            ).fetchone()[0]
            relation_active = conn.execute(
                "SELECT is_active FROM discovery_relation"
            ).fetchone()[0]
            self.assertEqual(host_active, 1)
            self.assertEqual(unit_active, 0)
            self.assertEqual(relation_active, 0)

    def test_missing_command_creates_coverage_gap(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = MODULE.open_db(db_path)
            result = MODULE.store_capture(
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
            capture_id = result["capture_ids"][0]
            row = conn.execute(
                "SELECT natural_key, is_active FROM discovery_entity WHERE kind = 'coverage_gap'"
            ).fetchone()
            self.assertEqual(row, ("coverage_gap:local:containers:docker", 1))
            evidence = conn.execute(
                "SELECT capture_id FROM discovery_evidence"
            ).fetchone()[0]
            self.assertEqual(evidence, capture_id)

    def test_full_sweep_only_deactivates_rows_for_same_skill(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = MODULE.open_db(db_path)
            MODULE.store_graph(
                conn,
                {
                    "run_id": "run-discovery-001",
                    "skill_key": "discovery_graph",
                    "status": "normalized",
                    "full_sweep": True,
                    "entities": [
                        {"kind": "host", "natural_key": "host:a", "title": "a", "attrs": {}},
                        {"kind": "repo", "natural_key": "repo:/tmp/app", "title": "/tmp/app", "attrs": {}},
                    ],
                    "relations": [],
                    "evidence": [],
                },
            )
            MODULE.store_graph(
                conn,
                {
                    "run_id": "run-reliability-001",
                    "skill_key": "reliability_ops",
                    "status": "normalized",
                    "full_sweep": True,
                    "entities": [
                        {
                            "kind": "health_assessment",
                            "natural_key": "health_assessment:host:a",
                            "title": "health",
                            "attrs": {},
                        }
                    ],
                    "relations": [],
                    "evidence": [],
                },
            )
            rows = conn.execute(
                "SELECT natural_key, is_active FROM discovery_entity ORDER BY natural_key"
            ).fetchall()
            self.assertEqual(
                rows,
                [
                    ("health_assessment:host:a", 1),
                    ("host:a", 1),
                    ("repo:/tmp/app", 1),
                ],
            )


if __name__ == "__main__":
    unittest.main()
