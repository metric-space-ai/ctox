#!/usr/bin/env python3
import importlib.util
import json
import sqlite3
import tempfile
import unittest
from pathlib import Path


def load_module(path: Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(module)
    return module


ROOT = Path(__file__).resolve().parents[2]
STORE = load_module(ROOT / "discovery-graph" / "scripts" / "discovery_store.py", "shared_store")
BOOTSTRAP = load_module(Path(__file__).resolve().parent / "deployment_bootstrap.py", "deployment_bootstrap")


class DeploymentBootstrapTest(unittest.TestCase):
    def test_builds_preflight_plan(self):
        with tempfile.TemporaryDirectory() as tmp:
            db_path = Path(tmp) / "deployment.db"
            conn = STORE.open_db(str(db_path))
            payload = {
                "skill_key": "service_deployment",
                "run_id": "run-test",
                "collectors": [
                    {
                        "collector": "package_managers",
                        "captures": [
                            {
                                "tool": "bash",
                                "argv": ["bash", "-lc", "command -v apt || true"],
                                "stdout": "/usr/bin/apt\n/usr/bin/snap\n",
                                "stderr": "",
                                "exit_code": 0,
                            }
                        ],
                    }
                ],
            }
            STORE.store_capture(conn, payload, "local")
            graph = BOOTSTRAP.build_graph(sqlite3.connect(db_path), "run-test")
            kinds = {item["kind"] for item in graph["entities"]}
            self.assertIn("deployment_preflight", kinds)
            self.assertIn("deployment_plan", kinds)
            plan = next(item for item in graph["entities"] if item["kind"] == "deployment_plan")
            self.assertIn("apt", plan["attrs"]["preferred_paths"])


if __name__ == "__main__":
    unittest.main()
