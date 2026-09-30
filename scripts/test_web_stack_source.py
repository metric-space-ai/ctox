#!/usr/bin/env python3
"""Source selection/regression tests: no dependency fetch, build, or browser."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("binding", Path(__file__).with_name("web_stack_source.py"))
binding = importlib.util.module_from_spec(spec)
spec.loader.exec_module(binding)

SHA = "b0b2f0e0437196f3a2b65f176d24543a143b2356"


class SourceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.canonical = self.root / "cargo/git/checkouts/workjet/native/web-stack/Cargo.toml"
        self.canonical.parent.mkdir(parents=True)
        self.canonical.write_text("[package]\nname='ctox-web-stack'\n")
        self.metadata = {
            "packages": [
                {"id": "daemon", "name": "ctox", "manifest_path": str(self.root / "Cargo.toml")},
                {"id": "pinned", "name": "ctox-web-stack", "manifest_path": str(self.canonical),
                 "source": "git+https://github.com/metric-space-ai/workjet?rev=" + SHA + "#" + SHA},
                {"id": "mirror", "name": "ctox-web-stack", "source": None,
                 "manifest_path": str(self.root / "src/tools/web-stack/Cargo.toml")},
            ],
            "resolve": {"nodes": [{"id": "daemon", "deps": [{"name": "ctox_web_stack", "pkg": "pinned"}]}]},
            "target_directory": str(self.root / "output"),
        }

    def selected(self):
        return binding.select_dependency(self.metadata, self.root)

    def test_direct_edge_selects_git_not_same_name_mirror(self):
        self.assertEqual(self.selected()["manifest"], str(self.canonical))
        self.assertEqual(self.selected()["revision"], SHA)

    def test_revision_changes_are_visible(self):
        changed = "a" * 40
        self.metadata["packages"][1]["source"] = "git+https://github.com/metric-space-ai/workjet#" + changed
        self.assertEqual(self.selected()["revision"], changed)

    def test_missing_or_ambiguous_direct_dependency_fails(self):
        node = self.metadata["resolve"]["nodes"][0]
        original = copy.deepcopy(node["deps"])
        for edges in ([], original + original):
            node["deps"] = edges
            with self.assertRaises(binding.BindingError):
                self.selected()

    def test_path_registry_and_unresolved_revision_fail(self):
        for source in (None, "registry+https://example.test", "git+https://example.test#main"):
            self.metadata["packages"][1]["source"] = source
            with self.assertRaises(binding.BindingError):
                self.selected()

    def test_git_label_on_mirror_manifest_is_not_proof(self):
        self.metadata["packages"][1]["manifest_path"] = self.metadata["packages"][2]["manifest_path"]
        with self.assertRaises(binding.BindingError):
            self.selected()

    def test_source_credentials_not_logged(self):
        self.metadata["packages"][1]["source"] = "git+https://user:secret@example.test/repo?token=private#" + SHA
        self.assertEqual(self.selected()["source"], "https://example.test/repo#" + SHA)

    def test_metadata_error_has_no_mirror_fallback(self):
        with patch.object(binding, "capture", side_effect=binding.BindingError("resolution failed")):
            with self.assertRaisesRegex(binding.BindingError, "resolution failed"):
                binding.resolve(self.root)

    def test_modified_cargo_git_checkout_fails(self):
        for head, dirty in (("a" * 40, ""), (SHA, " M src/lib.rs\n")):
            with patch.object(binding, "capture", side_effect=[json.dumps(self.metadata), head, dirty]):
                with self.assertRaises(binding.BindingError):
                    binding.resolve(self.root)
        with patch.object(binding, "capture", side_effect=[json.dumps(self.metadata), SHA, ""]):
            self.assertEqual(binding.resolve(self.root)["revision"], SHA)

    def test_focused_args_and_test_filter_preserved(self):
        args = ["--lib", "unlock::", "--quiet", "--", "--test-threads=2"]
        cmd = binding.focused_command(self.selected(), "test", args)
        self.assertEqual(cmd, ["cargo", "test", "--locked", "--manifest-path", str(self.canonical)] + args)

    def test_selector_and_job_overrides_fail(self):
        for arg in ("--manifest-path=x", "-pother", "--workspace", "-j8", "--config=x"):
            with self.assertRaises(binding.BindingError):
                binding.focused_command(self.selected(), "build", [arg])
        with patch.dict(os.environ, {"CARGO_BUILD_JOBS": "8"}):
            with self.assertRaises(binding.BindingError):
                binding.bounded_env()

    def test_stage2_rejects_uncompiled_mirror_before_mutation(self):
        with self.assertRaisesRegex(binding.BindingError, "local stealth mutation is not compiled"):
            binding.check_mutation_asset(self.root, self.selected())
        self.assertEqual(self.canonical.read_text(), "[package]\nname='ctox-web-stack'\n")

    def test_binary_receipt_rejects_stale_source_and_changed_binary(self):
        selected = self.selected()
        executable = self.root / "ctox"
        executable.write_bytes(b"built daemon")
        state = {"head": "test-head"}
        receipt = {"binding": selected, "root": str(self.root), "state": state,
                   "executable": str(executable), "sha256": binding.digest_file(executable)}
        receipt_file = binding.receipt_path(selected)
        receipt_file.parent.mkdir()
        with patch.object(binding, "source_state", return_value=state):
            with self.assertRaises(binding.BindingError):
                binding.verified_binary(self.root, selected)
            receipt_file.write_text(json.dumps(receipt))
            self.assertEqual(binding.verified_binary(self.root, selected), executable)
            executable.write_bytes(b"other daemon")
            with self.assertRaises(binding.BindingError):
                binding.verified_binary(self.root, selected)
        with patch.object(binding, "source_state", return_value={"head": "new-head"}):
            with self.assertRaises(binding.BindingError):
                binding.verified_binary(self.root, selected)

    def test_failed_build_removes_old_receipt(self):
        selected = self.selected()
        receipt = binding.receipt_path(selected)
        receipt.parent.mkdir()
        receipt.write_text("old proof")
        process = unittest.mock.Mock()
        process.stdout = []
        process.wait.return_value = 7
        with patch.object(binding, "source_state", return_value={}), patch.object(binding.subprocess, "Popen", return_value=process):
            self.assertEqual(binding.build_daemon(self.root, selected), 7)
        self.assertFalse(receipt.exists())

    def test_focused_failure_exit_code_preserved(self):
        with patch.object(binding, "resolve", return_value=self.selected()), patch.object(binding.subprocess, "call", return_value=9):
            self.assertEqual(binding.main(["test", "--", "--lib"]), 9)


    def test_success_cannot_hide_changed_source_binding(self):
        selected = self.selected()
        changed = dict(selected, revision="a" * 40)
        with patch.object(binding, "resolve", side_effect=[selected, changed]), patch.object(binding.subprocess, "call", return_value=0):
            self.assertEqual(binding.main(["test", "--", "--lib"]), 1)

    def test_receipt_source_state_changes_on_tracked_or_untracked_edits(self):
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        tracked = self.root / "source.rs"
        tracked.write_text("original")
        subprocess.run(["git", "add", "source.rs"], cwd=self.root, check=True)
        subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.test",
                        "commit", "--quiet", "-m", "fixture"], cwd=self.root, check=True)
        before = binding.source_state(self.root)
        tracked.write_text("modified")
        self.assertNotEqual(binding.source_state(self.root), before)
        tracked.write_text("original")
        (self.root / "new.rs").write_text("new source")
        self.assertNotEqual(binding.source_state(self.root), before)

if __name__ == "__main__":
    unittest.main()
