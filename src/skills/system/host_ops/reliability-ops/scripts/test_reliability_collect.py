#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


MODULE_PATH = pathlib.Path(__file__).with_name("reliability_collect.py")
SPEC = importlib.util.spec_from_file_location("reliability_collect", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class ReliabilityCollectTests(unittest.TestCase):
    def test_list_collectors_contains_expected_names(self):
        specs = MODULE.collector_specs(None, None)
        self.assertIn("cpu_memory", specs)
        self.assertIn("disk_io", specs)
        self.assertIn("network_pressure", specs)
        self.assertIn("service_status", specs)
        self.assertIn("service_logs", specs)
        self.assertIn("gpu_status", specs)

    def test_endpoint_collector_requires_url_to_exist(self):
        without_url = MODULE.collector_specs(None, None)
        with_url = MODULE.collector_specs(None, "https://example.com/health")
        self.assertNotIn("endpoint_probe", without_url)
        self.assertIn("endpoint_probe", with_url)


if __name__ == "__main__":
    unittest.main()
