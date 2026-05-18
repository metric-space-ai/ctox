#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


MODULE_PATH = pathlib.Path(__file__).with_name("change_collect.py")
SPEC = importlib.util.spec_from_file_location("change_collect", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class ChangeCollectTests(unittest.TestCase):
    def test_collectors_cover_scope_diff_and_verify(self):
        specs = MODULE.collector_specs("demo.service", "/tmp/repo", "/etc/hosts", "https://example.com/health")
        self.assertIn("change_scope", specs)
        self.assertIn("change_diff", specs)
        self.assertIn("change_verify", specs)


if __name__ == "__main__":
    unittest.main()
