#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import subprocess
import unittest
from unittest import mock


MODULE_PATH = pathlib.Path(__file__).with_name("capture_run.py")
SPEC = importlib.util.spec_from_file_location("capture_run", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class CaptureRunTests(unittest.TestCase):
    def test_run_json_raises_on_failure(self):
        with mock.patch.object(
            subprocess,
            "run",
            return_value=subprocess.CompletedProcess(["x"], 1, "", "boom"),
        ):
            with self.assertRaises(SystemExit):
                MODULE.run_json(["python3", "fake.py"])

    def test_main_runs_collect_then_store(self):
        raw_payload = {"run_id": "run-1", "collector": "host_identity", "captures": []}
        store_payload = {"run_id": "run-1", "capture_ids": ["capture-1"]}

        calls = []

        def fake_run(argv, input=None, capture_output=None, text=None, check=None):
            calls.append((argv, input))
            joined = " ".join(argv)
            if "linux_collect.py" in joined:
                return subprocess.CompletedProcess(argv, 0, json.dumps(raw_payload), "")
            return subprocess.CompletedProcess(argv, 0, json.dumps(store_payload), "")

        with mock.patch.object(subprocess, "run", side_effect=fake_run):
            with mock.patch("sys.argv", ["capture_run.py", "--db", "/tmp/test.db", "--repo-root", "/repo"]):
                with mock.patch("sys.stdout.write") as stdout_write:
                    rc = MODULE.main()
        self.assertEqual(rc, 0)
        self.assertEqual(len(calls), 3)
        rendered = "".join(part[0][0] for part in stdout_write.call_args_list if part[0])
        self.assertIn("run-1", rendered)


if __name__ == "__main__":
    unittest.main()
