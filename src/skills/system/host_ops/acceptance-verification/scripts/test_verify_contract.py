#!/usr/bin/env python3
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parent / "verify_contract.py"


class VerifyContractTests(unittest.TestCase):
    def run_script(self, payload, *extra_args):
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
            json.dump(payload, handle)
            temp_path = handle.name
        completed = subprocess.run(
            ["python3", str(SCRIPT), "--checks-json", temp_path, *extra_args],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        return json.loads(completed.stdout)

    def test_returns_needs_repair_when_higher_layer_fails(self):
        payload = [
            {"layer": "service_process", "ok": True},
            {"layer": "listener", "ok": True},
            {"layer": "http", "ok": True},
            {"layer": "authenticated_api", "ok": False, "cause": "secret_invalid", "detail": "401"},
        ]
        result = self.run_script(payload)
        self.assertEqual(result["state"], "needs_repair")
        self.assertEqual(result["failed_layer"]["layer"], "authenticated_api")
        self.assertEqual(result["failed_layer"]["cause"], "secret_invalid")

    def test_returns_executed_when_all_known_layers_pass(self):
        payload = [
            {"layer": "service_process", "ok": True},
            {"layer": "listener", "ok": True},
            {"layer": "http", "ok": True},
            {"layer": "authenticated_api", "ok": True},
            {"layer": "admin_identity", "ok": True},
        ]
        result = self.run_script(payload)
        self.assertEqual(result["state"], "executed")
        self.assertIsNone(result["failed_layer"])

    def test_operator_managed_profile_requires_authenticated_api(self):
        payload = [
            {"layer": "service_process", "ok": True},
            {"layer": "listener", "ok": True},
            {"layer": "http", "ok": True},
        ]
        result = self.run_script(payload, "--required-profile", "operator_managed")
        self.assertEqual(result["state"], "needs_repair")
        self.assertEqual(result["failed_layer"]["layer"], "authenticated_api")
        self.assertEqual(result["failed_layer"]["cause"], "verification_incomplete")

    def test_safe_mutation_profile_requires_mutating_smoke(self):
        payload = [
            {"layer": "service_process", "ok": True},
            {"layer": "listener", "ok": True},
            {"layer": "http", "ok": True},
            {"layer": "authenticated_api", "ok": True},
            {"layer": "admin_identity", "ok": True},
        ]
        result = self.run_script(payload, "--required-profile", "safe_mutation")
        self.assertEqual(result["state"], "needs_repair")
        self.assertEqual(result["failed_layer"]["layer"], "mutating_smoke")

    def test_explicit_minimum_layer_can_require_persistence(self):
        payload = [
            {"layer": "service_process", "ok": True},
            {"layer": "listener", "ok": True},
            {"layer": "http", "ok": True},
            {"layer": "authenticated_api", "ok": True},
            {"layer": "admin_identity", "ok": True},
            {"layer": "mutating_smoke", "ok": True},
        ]
        result = self.run_script(payload, "--minimum-layer", "persistence")
        self.assertEqual(result["state"], "needs_repair")
        self.assertEqual(result["failed_layer"]["layer"], "persistence")

    def test_read_only_service_profile_allows_http_as_completion(self):
        payload = [
            {"layer": "service_process", "ok": True},
            {"layer": "listener", "ok": True},
            {"layer": "http", "ok": True},
        ]
        result = self.run_script(payload, "--required-profile", "read_only_service")
        self.assertEqual(result["state"], "executed")
        self.assertEqual(result["required_minimum_layer"], "http")


if __name__ == "__main__":
    unittest.main()
