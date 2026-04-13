#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


MODULE_PATH = pathlib.Path(__file__).with_name("incident_collect.py")
SPEC = importlib.util.spec_from_file_location("incident_collect", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class IncidentCollectTests(unittest.TestCase):
    def test_collectors_expand_with_optional_scope(self):
        base = MODULE.collector_specs(None, None, None, None)
        scoped = MODULE.collector_specs("demo.service", "https://example.com", "example.com", "/tmp/repo")
        self.assertIn("incident_overview", base)
        self.assertIn("incident_logs", base)
        self.assertIn("recent_change", base)
        self.assertIn("incident_service", scoped)
        self.assertIn("dependency_probe", scoped)


if __name__ == "__main__":
    unittest.main()
