#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


MODULE_PATH = pathlib.Path(__file__).with_name("security_collect.py")
SPEC = importlib.util.spec_from_file_location("security_collect", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class SecurityCollectTests(unittest.TestCase):
    def test_collectors_cover_accounts_listeners_and_permissions(self):
        specs = MODULE.collector_specs("demo.service", "/tmp/cert.pem", "/etc", "example.com", "443")
        self.assertIn("accounts", specs)
        self.assertIn("listeners", specs)
        self.assertIn("permissions", specs)


if __name__ == "__main__":
    unittest.main()
