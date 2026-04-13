#!/usr/bin/env python3
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parent / "secret_material.py"


class SecretMaterialTest(unittest.TestCase):
    def test_upsert_env_and_describe(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "runtime" / "secrets" / "demo.env"
            result = subprocess.check_output(
                [
                    "python3",
                    str(SCRIPT),
                    "upsert-env",
                    "--path",
                    str(target),
                    "--set",
                    "DEMO_USER=admin",
                    "--set",
                    "DEMO_PASSWORD=secret",
                ],
                text=True,
            )
            payload = json.loads(result)
            self.assertTrue(payload["exists"])
            self.assertEqual(payload["keys"], ["DEMO_PASSWORD", "DEMO_USER"])
            self.assertEqual(oct(target.stat().st_mode & 0o777), "0o600")

    def test_upsert_metadata_and_describe(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "runtime" / "secrets" / "catalog.json"
            subprocess.check_output(
                [
                    "python3",
                    str(SCRIPT),
                    "upsert-metadata",
                    "--path",
                    str(target),
                    "--secret-key",
                    "nextcloud_admin",
                    "--kind",
                    "generated",
                    "--status",
                    "present",
                    "--reply-path",
                    "tui_only",
                    "--material-path",
                    "/tmp/nextcloud.env",
                    "--binding",
                    "service:nextcloud",
                    "--binding",
                    "deployment:local_install",
                ],
                text=True,
            )
            result = subprocess.check_output(
                [
                    "python3",
                    str(SCRIPT),
                    "describe-metadata",
                    "--path",
                    str(target),
                ],
                text=True,
            )
            payload = json.loads(result)
            self.assertEqual(len(payload), 1)
            self.assertEqual(payload[0]["secret_key"], "nextcloud_admin")
            self.assertEqual(payload[0]["reply_path"], "tui_only")
            self.assertIn("service:nextcloud", payload[0]["bindings"])
            self.assertEqual(oct(target.stat().st_mode & 0o777), "0o600")


if __name__ == "__main__":
    unittest.main()
