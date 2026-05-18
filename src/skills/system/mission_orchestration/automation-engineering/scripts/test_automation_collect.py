#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


MODULE_PATH = pathlib.Path(__file__).with_name("automation_collect.py")
SPEC = importlib.util.spec_from_file_location("automation_collect", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class AutomationCollectTests(unittest.TestCase):
    def test_collectors_cover_ctox_and_repo(self):
        specs = MODULE.collector_specs("/tmp/repo")
        self.assertIn("ctox_state", specs)
        self.assertIn("repo_scripts", specs)


if __name__ == "__main__":
    unittest.main()
