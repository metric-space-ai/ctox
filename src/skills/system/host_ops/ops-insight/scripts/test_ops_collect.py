#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


MODULE_PATH = pathlib.Path(__file__).with_name("ops_collect.py")
SPEC = importlib.util.spec_from_file_location("ops_collect", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class OpsCollectTests(unittest.TestCase):
    def test_collectors_cover_ctox_host_and_kernel(self):
        specs = MODULE.collector_specs("/tmp/ops.db")
        self.assertIn("ctox_state", specs)
        self.assertIn("host_brief", specs)
        self.assertIn("kernel_summary", specs)


if __name__ == "__main__":
    unittest.main()
