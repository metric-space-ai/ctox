#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


MODULE_PATH = pathlib.Path(__file__).with_name("recovery_collect.py")
SPEC = importlib.util.spec_from_file_location("recovery_collect", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(MODULE)


class RecoveryCollectTests(unittest.TestCase):
    def test_collectors_cover_scheduler_and_backup_shapes(self):
        specs = MODULE.collector_specs("backup.service", "/tmp/backup.tar", "/tmp/repo", "/tmp/db.dump")
        self.assertIn("scheduler", specs)
        self.assertIn("filesystem_backup", specs)
        self.assertIn("snapshot_repo", specs)


if __name__ == "__main__":
    unittest.main()
