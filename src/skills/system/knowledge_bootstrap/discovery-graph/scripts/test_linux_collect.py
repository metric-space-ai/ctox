#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import unittest
from unittest import mock


MODULE_PATH = pathlib.Path(__file__).with_name("linux_collect.py")
SPEC = importlib.util.spec_from_file_location("linux_collect", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class LinuxCollectTests(unittest.TestCase):
    def test_list_collectors_contains_expected_names(self):
        payload = MODULE.list_collectors(None)
        self.assertIn("host_identity", payload["collectors"])
        self.assertIn("network", payload["collectors"])
        self.assertIn("listeners", payload["collectors"])
        self.assertIn("services", payload["collectors"])
        self.assertIn("journals", payload["collectors"])
        self.assertIn("processes", payload["collectors"])
        self.assertIn("storage", payload["collectors"])
        self.assertIn("containers", payload["collectors"])
        self.assertIn("kubernetes", payload["collectors"])

    def test_repo_inventory_only_exists_with_repo_root(self):
        without_repo = MODULE.collector_specs(None)
        with_repo = MODULE.collector_specs(".")
        self.assertNotIn("repo_inventory", without_repo)
        self.assertIn("repo_inventory", with_repo)

    def test_services_collector_includes_show_commands(self):
        specs = MODULE.collector_specs(None)
        service_argv = [" ".join(item["argv"]) for item in specs["services"]]
        self.assertTrue(any("systemctl show --type=service" in item for item in service_argv))
        self.assertTrue(any("systemctl show --type=timer" in item for item in service_argv))
        self.assertTrue(any("systemctl --user list-units" in item for item in service_argv))
        self.assertTrue(any("systemctl --user list-timers" in item for item in service_argv))

    def test_processes_collector_includes_cgroup_capture(self):
        specs = MODULE.collector_specs(None)
        process_argv = [" ".join(item["argv"]) for item in specs["processes"]]
        self.assertTrue(any("ps -eo" in item for item in process_argv))
        self.assertTrue(any("/proc/$pid/cgroup" in item for item in process_argv))

    def test_services_capture_expands_to_per_unit_show_calls(self):
        def fake_run(spec):
            argv = spec["argv"]
            if argv[:2] == ["systemctl", "list-units"]:
                return {
                    "tool": "systemctl",
                    "argv": argv,
                    "stdout": "ssh.service loaded active running OpenSSH\n",
                    "stderr": "",
                    "exit_code": 0,
                }
            if argv[:3] == ["systemctl", "--user", "list-units"]:
                return {
                    "tool": "systemctl",
                    "argv": argv,
                    "stdout": "cto-agent.service loaded active running CTO Agent\n",
                    "stderr": "",
                    "exit_code": 0,
                }
            if argv[:2] == ["systemctl", "list-timers"]:
                return {
                    "tool": "systemctl",
                    "argv": argv,
                    "stdout": "Thu 2026-03-26 00:00:00 CET  1h left  n/a  n/a  backup.timer  backup.service\n",
                    "stderr": "",
                    "exit_code": 0,
                }
            return {
                "tool": spec["tool"],
                "argv": argv,
                "stdout": "",
                "stderr": "",
                "exit_code": 0,
            }

        commands = [
            {"tool": "systemctl", "argv": ["systemctl", "list-units", "--type=service"]},
            {"tool": "systemctl", "argv": ["systemctl", "--user", "list-units", "--type=service"]},
            {"tool": "systemctl", "argv": ["systemctl", "list-timers", "--all"]},
        ]
        with mock.patch.object(MODULE, "run_command", side_effect=fake_run):
            payload = MODULE.build_capture("services", None, commands, run_id="run-1")
        argvs = [capture["argv"] for capture in payload["captures"]]
        self.assertIn(
            [
                "systemctl",
                "show",
                "ssh.service",
                "--property",
                "Id,Names,LoadState,ActiveState,SubState,MainPID,FragmentPath,Description",
            ],
            argvs,
        )
        self.assertIn(
            [
                "systemctl",
                "--user",
                "show",
                "cto-agent.service",
                "--property",
                "Id,Names,LoadState,ActiveState,SubState,MainPID,FragmentPath,Description",
            ],
            argvs,
        )
        self.assertIn(
            [
                "systemctl",
                "show",
                "backup.timer",
                "--property",
                "Id,Names,Unit,NextElapseUSecRealtime,LastTriggerUSec,FragmentPath,Description",
            ],
            argvs,
        )

    def test_capture_shape_is_json_serializable(self):
        specs = [{"tool": "missing", "argv": ["definitely-not-a-real-command"]}]
        payload = MODULE.build_capture("missing", None, specs)
        json.dumps(payload)
        self.assertEqual(payload["collector"], "missing")
        self.assertEqual(len(payload["captures"]), 1)

    def test_run_all_uses_shared_run_id(self):
        specs = MODULE.collector_specs(".")
        captured_at = "2026-03-25T00:00:00Z"
        run_id = MODULE.stable_run_id(".", captured_at)
        combined = {
            "model_version": 1,
            "run_id": run_id,
            "captured_at": captured_at,
            "repo_root": ".",
            "collectors": [
                MODULE.build_capture(name, ".", commands, run_id=run_id)
                for name, commands in specs.items()
            ],
        }
        child_run_ids = {item["run_id"] for item in combined["collectors"]}
        self.assertEqual(child_run_ids, {run_id})


if __name__ == "__main__":
    unittest.main()
