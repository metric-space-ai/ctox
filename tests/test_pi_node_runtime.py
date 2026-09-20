"""Offline installer regressions; no Node download, bundle execution or network."""
import importlib.util
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SOURCE = Path(__file__).resolve().parents[1] / "src/core/coding_agents/install_node_runtime.py"
spec = importlib.util.spec_from_file_location("runtime_installer", SOURCE)
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)


class RuntimeInstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.name = "node-v" + runtime.VERSION + "-linux-x64"
        self.bundle = self.root / "bundle.mjs"
        self.bundle.write_text("export const inert = true;")
        self.bundle_sha = runtime.digest(self.bundle)

    def archive(self, entries=None):
        path = self.root / "node.tar.xz"
        if entries is None:
            entries = [(self.name + "/bin/node", b"fixture-node", None)]
        with tarfile.open(path, "w:xz") as stream:
            for name, data, link in entries:
                info = tarfile.TarInfo(name)
                info.mode = 0o755
                if link is not None:
                    info.type = tarfile.SYMTYPE
                    info.linkname = link
                    stream.addfile(info)
                else:
                    info.size = len(data)
                    stream.addfile(info, io.BytesIO(data))
        return path

    def provision(self, archive):
        return runtime.provision(self.root, archive, self.bundle, self.bundle_sha, "x86_64")

    def test_bad_hash_cannot_extract_or_execute(self):
        archive = self.archive()
        with patch.object(runtime, "preflight") as preflight:
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                self.provision(archive)
            preflight.assert_not_called()
        self.assertFalse((self.root / self.name).exists())
        self.assertFalse(list(self.root.glob(".node-install-*")))

    def test_atomic_publish_reuse_and_tamper_rejection(self):
        archive = self.archive()
        pin = {"x86_64": ("x64", runtime.digest(archive))}
        with patch.object(runtime, "ARCHIVES", pin), patch.object(runtime, "preflight") as check:
            node = self.provision(archive)
            self.assertEqual(node, self.root / self.name / "bin/node")
            self.assertEqual(self.provision(None), node)
            self.assertEqual(check.call_count, 2)
            receipt = json.loads((node.parent.parent / "ctox-runtime-receipt.json").read_text())
            self.assertEqual(receipt["node_sha256"], runtime.digest(node))
            node.write_text("tampered")
            with self.assertRaisesRegex(ValueError, "installed Node checksum"):
                self.provision(None)
            self.assertEqual(check.call_count, 2)

    def test_import_failure_does_not_publish(self):
        archive = self.archive()
        with patch.object(runtime, "ARCHIVES", {"x86_64": ("x64", runtime.digest(archive))}), \
             patch.object(runtime, "preflight", side_effect=ValueError("import failed")):
            with self.assertRaisesRegex(ValueError, "import failed"):
                self.provision(archive)
        self.assertFalse((self.root / self.name).exists())
        self.assertFalse(list(self.root.glob(".node-install-*")))

    def test_archive_escape_and_symlink_parent_rejected_before_write(self):
        cases = [
            [(self.name + "/../escape", b"bad", None)],
            [(self.name + "/bin", b"", "../../outside")],
            [(self.name + "/bin", b"", "lib"), (self.name + "/bin/node", b"bad", None)],
            [(self.name + "/bin/node", b"one", None), (self.name + "/bin/node", b"two", None)],
        ]
        for entries in cases:
            with self.subTest(entries=entries):
                with self.assertRaises(ValueError):
                    runtime.unpack(self.archive(entries), self.root, self.name)
                self.assertFalse((self.root / self.name).exists())

    def test_relative_npm_link_preserved_inside_tree(self):
        archive = self.archive([
            (self.name + "/lib/node_modules/npm/bin/npm-cli.js", b"fixture", None),
            (self.name + "/bin/npm", b"", "../lib/node_modules/npm/bin/npm-cli.js"),
        ])
        runtime.unpack(archive, self.root, self.name)
        self.assertEqual((self.root / self.name / "bin/npm").read_bytes(), b"fixture")

    def test_preflight_checks_bundle_and_clears_main_entry(self):
        node = self.root / "node"
        node.write_text("not executed")
        with patch.object(runtime, "bounded", return_value=("v" + runtime.VERSION + "\n").encode()) as run:
            runtime.preflight(node, self.bundle, self.bundle_sha, self.root)
            self.assertEqual(run.call_count, 2)
            self.assertIn("process.argv[1]=undefined", run.call_args_list[1].args[1][2])
            self.bundle.write_text("changed")
            with self.assertRaisesRegex(ValueError, "bundle checksum"):
                runtime.preflight(node, self.bundle, self.bundle_sha, self.root)
            self.assertEqual(run.call_count, 2)

    def test_bounded_child_has_no_inherited_secret_or_preload(self):
        # Shell fixture only exercises process environment, never a fake Node installation.
        script = self.root / "probe"
        script.write_text('#!/bin/sh\ntest -z "$CTOX_TEST_SECRET" && test -z "$NODE_OPTIONS"\n')
        script.chmod(0o755)
        with patch.dict(os.environ, {"CTOX_TEST_SECRET": "fixture", "NODE_OPTIONS": "--bad"}):
            runtime.bounded(script, [], self.root)


if __name__ == "__main__":
    unittest.main()
