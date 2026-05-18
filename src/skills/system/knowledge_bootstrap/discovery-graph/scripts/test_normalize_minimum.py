#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile
import unittest
import sqlite3


STORE_PATH = pathlib.Path(__file__).with_name("discovery_store.py")
STORE_SPEC = importlib.util.spec_from_file_location("discovery_store", STORE_PATH)
STORE_MODULE = importlib.util.module_from_spec(STORE_SPEC)
assert STORE_SPEC and STORE_SPEC.loader
STORE_SPEC.loader.exec_module(STORE_MODULE)

NORMALIZE_PATH = pathlib.Path(__file__).with_name("normalize_minimum.py")
NORMALIZE_SPEC = importlib.util.spec_from_file_location("normalize_minimum", NORMALIZE_PATH)
NORMALIZE_MODULE = importlib.util.module_from_spec(NORMALIZE_SPEC)
assert NORMALIZE_SPEC and NORMALIZE_SPEC.loader
NORMALIZE_SPEC.loader.exec_module(NORMALIZE_MODULE)


class NormalizeMinimumTests(unittest.TestCase):
    def test_build_graph_fails_without_captures(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = STORE_MODULE.open_db(db_path)
            STORE_MODULE.ensure_run(conn, "run-empty")
            with self.assertRaises(SystemExit):
                NORMALIZE_MODULE.build_graph(conn, "run-empty")

    def test_parse_service_blocks_handles_back_to_back_records(self):
        text = (
            "Id=one.service\nMainPID=11\nDescription=One\n"
            "Id=two.service\nMainPID=22\nDescription=Two\n"
        )
        blocks = NORMALIZE_MODULE.parse_service_blocks(text)
        self.assertEqual([block["Id"] for block in blocks], ["one.service", "two.service"])

    def test_build_graph_creates_minimum_entities_and_relations(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = str(pathlib.Path(tmp) / "graph.db")
            conn = STORE_MODULE.open_db(db_path)
            payload = {
                "run_id": "run-001",
                "collectors": [
                    {
                        "run_id": "run-001",
                        "collector": "host_identity",
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
                        "run_id": "run-001",
                        "collector": "processes",
                        "captures": [
                            {
                                "tool": "ps",
                                "argv": ["ps"],
                                "available": True,
                                "stdout": "123 1 root S 0.0 0.1 nginx nginx: master process\n",
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:01Z",
                                "finished_at": "2026-03-25T00:00:02Z",
                            },
                            {
                                "tool": "bash",
                                "argv": ["bash", "-lc", "for pid in $(ps -eo pid=); do ..."],
                                "available": True,
                                "stdout": "PID=123\n0::/system.slice/nginx.service\n\n",
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:01Z",
                                "finished_at": "2026-03-25T00:00:02Z",
                            }
                        ],
                    },
                    {
                        "run_id": "run-001",
                        "collector": "listeners",
                        "captures": [
                            {
                                "tool": "ss",
                                "argv": ["ss", "-tulpnH"],
                                "available": True,
                                "stdout": 'tcp LISTEN 0 511 0.0.0.0:80 0.0.0.0:* users:(("nginx",pid=123,fd=6))\n',
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:02Z",
                                "finished_at": "2026-03-25T00:00:03Z",
                            }
                        ],
                    },
                    {
                        "run_id": "run-001",
                        "collector": "services",
                        "captures": [
                            {
                                "tool": "systemctl",
                                "argv": ["systemctl", "list-units", "--type=service"],
                                "available": True,
                                "stdout": "nginx.service  loaded  active  running  Nginx\n",
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:03Z",
                                "finished_at": "2026-03-25T00:00:04Z",
                            },
                            {
                                "tool": "systemctl",
                                "argv": [
                                    "systemctl",
                                    "show",
                                    "nginx.service",
                                    "--property",
                                    "Id,Names,LoadState,ActiveState,SubState,MainPID,FragmentPath,Description",
                                ],
                                "available": True,
                                "stdout": "Id=nginx.service\nMainPID=123\nFragmentPath=/repo/services/nginx.service\nDescription=Nginx\n\n",
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:03Z",
                                "finished_at": "2026-03-25T00:00:04Z",
                            },
                            {
                                "tool": "systemctl",
                                "argv": ["systemctl", "list-timers", "--all"],
                                "available": True,
                                "stdout": "Thu 2026-03-26 00:00:00 CET  1h left  n/a  n/a  backup.timer  backup.service\n",
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:04Z",
                                "finished_at": "2026-03-25T00:00:05Z",
                            },
                            {
                                "tool": "systemctl",
                                "argv": [
                                    "systemctl",
                                    "show",
                                    "backup.timer",
                                    "--property",
                                    "Id,Names,Unit,NextElapseUSecRealtime,LastTriggerUSec,FragmentPath,Description",
                                ],
                                "available": True,
                                "stdout": "Id=backup.timer\nUnit=backup.service\nDescription=Backup timer\n\n",
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:05Z",
                                "finished_at": "2026-03-25T00:00:06Z",
                            },
                        ],
                    },
                    {
                        "run_id": "run-001",
                        "collector": "repo_inventory",
                        "repo_root": "/repo",
                        "captures": [
                            {
                                "tool": "rg",
                                "argv": ["rg", "--files"],
                                "available": True,
                                "stdout": "services/nginx.service\n",
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:05Z",
                                "finished_at": "2026-03-25T00:00:06Z",
                            },
                            {
                                "tool": "rg",
                                "argv": ["rg", "-n", "service"],
                                "available": True,
                                "stdout": "services/nginx.service:1:ExecStart=/usr/sbin/nginx\n",
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:06Z",
                                "finished_at": "2026-03-25T00:00:07Z",
                            },
                        ],
                    },
                    {
                        "run_id": "run-001",
                        "collector": "journals",
                        "captures": [
                            {
                                "tool": "journalctl",
                                "argv": ["journalctl"],
                                "available": True,
                                "stdout": "Mar 25 00:00:00 host nginx.service: bind() failed: Address already in use\n",
                                "stderr": "",
                                "exit_code": 0,
                                "started_at": "2026-03-25T00:00:07Z",
                                "finished_at": "2026-03-25T00:00:08Z",
                            }
                        ],
                    },
                ],
            }
            STORE_MODULE.store_capture(conn, payload, "/repo")
            graph = NORMALIZE_MODULE.build_graph(conn, "run-001")
            kinds = {item["kind"] for item in graph["entities"]}
            relations = {item["relation"] for item in graph["relations"]}
            self.assertIn("host", kinds)
            self.assertIn("process", kinds)
            self.assertIn("listener", kinds)
            self.assertIn("systemd_unit", kinds)
            self.assertIn("timer", kinds)
            self.assertIn("repo_file", kinds)
            self.assertIn("journal_finding", kinds)
            self.assertIn("managed_by", relations)
            self.assertIn("defined_in", relations)
            self.assertIn("scheduled_by", relations)
            self.assertIn("about", relations)
            process_to_unit = [
                item
                for item in graph["relations"]
                if item["relation"] == "managed_by" and item["from"]["kind"] == "process"
            ]
            self.assertTrue(process_to_unit)


if __name__ == "__main__":
    unittest.main()
