#!/usr/bin/env python3
"""Mutation runner contract tests; no compilation, network or real browser."""
import contextlib
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import web_stack_mutation as mutation


class BrowserPreparationTests(unittest.TestCase):
    def setUp(self):
        self.reference = Path("/isolated-browser-reference")
        self.report = {
            "ok": True, "tool": "ctox_browser_prepare",
            "doctor": {"reference_dir": str(self.reference), "automation_ready": True,
                       "runner_dependency_installed": True, "runner_browser_installed": True,
                       "smoke": {"ran": True, "ok": True}},
        }

    def check(self):
        return mutation.require_browser_preparation(json.dumps(self.report), self.reference)

    def test_actual_ready_report_with_passing_smoke_is_accepted(self):
        self.assertEqual(self.check()["smoke"], {"ran": True, "ok": True})

    def test_successful_cli_report_with_incomplete_prerequisites_is_rejected(self):
        for field in ("automation_ready", "runner_dependency_installed", "runner_browser_installed"):
            for value in (False, None, "true", 1):
                with self.subTest(field=field, value=value):
                    report = json.loads(json.dumps(self.report))
                    report["doctor"][field] = value
                    with self.assertRaisesRegex(mutation.BindingError, "prerequisites"):
                        mutation.require_browser_preparation(json.dumps(report), self.reference)

    def test_skipped_or_failed_smoke_is_not_a_positive_control(self):
        for smoke in (None, {"ran": False, "ok": True}, {"ran": True, "ok": False},
                      {"ran": True}, {"ran": 1, "ok": True}):
            with self.subTest(smoke=smoke):
                self.report["doctor"]["smoke"] = smoke
                with self.assertRaisesRegex(mutation.BindingError, "passing browser smoke"):
                    self.check()

    def test_other_reference_cannot_prove_the_isolated_fixture(self):
        self.report["doctor"]["reference_dir"] = "/another-browser-reference"
        with self.assertRaisesRegex(mutation.BindingError, "different reference"):
            self.check()

    def test_malformed_or_unrelated_success_reply_is_refused(self):
        for reply in ("not JSON", "[]", '{"ok":true}', '{"ok":true,"tool":"other","doctor":{}}'):
            with self.subTest(reply=reply), self.assertRaises(mutation.BindingError):
                mutation.require_browser_preparation(reply, self.reference)


class ProbeTests(unittest.TestCase):
    def test_metadata_target_is_reused_and_mac_system_disk_is_rejected(self):
        with patch.object(mutation.sys, "platform", "darwin"):
            target = "/Volumes/tmp/dev-artifacts/ctox/web-stack-source-binding/target"
            self.assertEqual(mutation.mutation_target_directory({"target_directory": target}), Path(target).resolve())
            for invalid in ("/tmp/ctox-target", "relative-target"):
                with self.assertRaises(mutation.BindingError):
                    mutation.mutation_target_directory({"target_directory": invalid})

    def test_copied_pdf_selector_keeps_original_git_identity_and_features(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "copied/Cargo.toml"
            manifest.parent.mkdir()
            original = '[features]\nfull = ["dep:ctox-pdf-parse"]\n[dependencies]\nctox-pdf-parse = { path = "../pdf-parse", optional = true }\n'
            manifest.write_text(original)
            revision = "b" * 40
            binding = {"manifest": str(Path(tmp) / "canonical/Cargo.toml"),
                       "source": "https://example.test/workjet#" + revision, "revision": revision}
            source = mutation.retain_pdf_git_dependency(manifest, binding)
            self.assertEqual(source, "git+https://example.test/workjet?rev=" + revision + "#" + revision)
            changed = manifest.read_text()
            self.assertIn('full = ["dep:ctox-pdf-parse"]', changed)
            self.assertIn('optional = true', changed)
            self.assertIn('git = "https://example.test/workjet"', changed)
            self.assertIn('rev = "' + revision + '"', changed)
            self.assertNotIn('path =', changed)

    def test_pdf_rewrite_refuses_canonical_checkout(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "Cargo.toml"
            original = 'ctox-pdf-parse = { path = "../pdf-parse", optional = true }'
            manifest.write_text(original)
            binding = {"manifest": str(manifest), "source": "https://example.test/workjet#" + "b" * 40, "revision": "b" * 40}
            with self.assertRaises(mutation.BindingError):
                mutation.retain_pdf_git_dependency(manifest, binding)
            self.assertEqual(manifest.read_text(), original)

    def test_missing_or_duplicate_pdf_selector_is_rejected_unchanged(self):
        with tempfile.TemporaryDirectory() as tmp:
            manifest = Path(tmp) / "Cargo.toml"
            binding = {"manifest": str(Path(tmp) / "original/Cargo.toml"), "source": "https://example.test/workjet#" + "b" * 40, "revision": "b" * 40}
            for text in ('[dependencies]\n', 'ctox-pdf-parse = { path = "../pdf-parse", optional = true }\n' * 2):
                manifest.write_text(text)
                with self.assertRaises(mutation.BindingError):
                    mutation.retain_pdf_git_dependency(manifest, binding)
                self.assertEqual(manifest.read_text(), text)

    def test_canonical_header_quotes_and_line_endings_are_preserved(self):
        for quote in ("'", '"'):
            for newline in ("\n", "\r\n"):
                with self.subTest(quote=quote, newline=repr(newline)):
                    header = "// Canonical source license header" + newline
                    opening = "(() => {" + newline + "  " + quote + "use strict" + quote + ";"
                    body = newline + "  globalThis.stealthApplied = true;" + newline + "})();"
                    original = (header + opening + body).encode()
                    self.assertEqual(mutation.mutated_bytes(original),
                                     (header + opening + newline + "  return; /* e2e-sabotage */" + body).encode())

    def test_mixed_quote_openings_are_still_ambiguous(self):
        original = (mutation.MARKER + "\n})();\n" + mutation.MARKER.replace("'", '"') + "\n})();").encode()
        with self.assertRaises(mutation.BindingError):
            mutation.mutated_bytes(original)

    def test_marker_requires_exactly_one_known_opening(self):
        original = (mutation.MARKER + "\n})();").encode()
        self.assertIn(b"return; /* e2e-sabotage */", mutation.mutated_bytes(original))
        for malformed in (b"different script", original + original):
            with self.assertRaises(mutation.BindingError):
                mutation.mutated_bytes(malformed)

    def test_negative_requires_real_test_failures(self):
        cases = [("not JSON", 1), ('{"error":"network timeout"}', 1),
                 ('{"probes":[]}', 1),
                 ('{"probes":[{"failed_count":true,"failed_tests":["webdriver"]}]}', 1),
                 ('{"probes":[{"failed_count":1,"failed_tests":[]}]}', 1),
                 ('{"probes":[{"failed_count":1,"failed_tests":["webdriver"]}]}', 0),
                 ('{"probes":[{"failed_count":0}]}', 1)]
        for stdout, code in cases:
            with self.subTest(stdout=stdout, code=code), self.assertRaises(mutation.BindingError):
                mutation.require_probe(stdout, code, True)
        valid = json.dumps({"probes": [{"failed_count": 1, "failed_tests": ["webdriver"]}]})
        self.assertEqual(mutation.require_probe(valid, 1, True)["failed_count"], 1)

    def test_positive_requires_zero_exit_and_zero_failures(self):
        payload = json.dumps({"probes": [{"failed_count": 0, "failed_tests": []}]})
        self.assertEqual(mutation.require_probe(payload, 0, False)["exit"], 0)
        with self.assertRaises(mutation.BindingError):
            mutation.require_probe(payload, 1, False)

    @unittest.skipUnless(os.name == "posix", "POSIX mutation copy preserves native symlinks")
    def test_local_copy_preserves_selected_old_revision_and_symlink(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "origin"
            root.mkdir()
            def git(*args):
                return subprocess.run(["git", *args], cwd=root, check=True, text=True,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout.strip()
            git("init", "--quiet")
            (root / "source").write_text("first")
            (root / "link").symlink_to("source")
            git("add", ".")
            git("-c", "user.name=Test", "-c", "user.email=test@example.test", "commit", "--quiet", "-m", "first")
            old = git("rev-parse", "HEAD")
            (root / "source").write_text("second")
            git("-c", "user.name=Test", "-c", "user.email=test@example.test",
                "commit", "--quiet", "-am", "second")
            current = git("rev-parse", "HEAD")
            copied = Path(tmp) / "copy"
            mutation.copy_revision(root, old, copied)
            self.assertEqual((copied / "source").read_text(), "first")
            self.assertTrue((copied / "link").is_symlink())
            self.assertEqual(subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=copied, text=True).strip(), old)
            self.assertEqual(git("rev-parse", "HEAD"), current)
            self.assertEqual(git("status", "--porcelain"), "")


class SidecarReuseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / "operator"
        self.isolated = self.base / "isolated"
        self.bundle = self.root / "src/core/coding_agents/pi-sidecar/dist/ctox-pi-sidecar.mjs"
        self.bundle.parent.mkdir(parents=True)
        self.bundle.write_bytes(b"actual generated bundle")
        self.proof = {"sha256": mutation.digest_file(self.bundle), "bytes": self.bundle.stat().st_size,
                      "sidecar_source_tree": "tree", "package_lock_blob": "lock",
                      "node_version": "v26.6.0", "npm_version": "11.18.0",
                      "official_build_script": True, "scripts_ignored_on_install": True}
        prefix = ["npm", "--prefix", str(self.bundle.parent.parent)]
        self.receipt = {"sidecar_build": self.proof, "stages": [
            {"stage": "00a-locked-sidecar-install", "exit": 0,
             "command": prefix + ["ci", "--ignore-scripts", "--no-audit", "--no-fund"]},
            {"stage": "00b-real-sidecar-bundle", "exit": 0, "command": prefix + ["run", "build"]}]}
        self.path = self.base / "receipt.json"

    def reuse(self, values=None):
        self.path.write_text(json.dumps(self.receipt))
        with patch.object(mutation, "capture", side_effect=values or
                          ["tree", "tree", "lock", "lock", "v26.6.0", "11.18.0"]):
            return mutation.reuse_sidecar_bundle(self.root, self.isolated, self.path,
                                                 mutation.time.monotonic() + 60)

    def test_same_input_official_bundle_is_copied_and_recorded(self):
        result = self.reuse()
        copied = self.isolated / self.bundle.relative_to(self.root)
        self.assertEqual(copied.read_bytes(), self.bundle.read_bytes())
        self.assertEqual(result["sha256"], self.proof["sha256"])
        self.assertEqual(result["receipt"], str(self.path.resolve()))
        # Unrelated checks may have failed; only the two actual build stages
        # establish this generated input's provenance.
        self.receipt["passed"] = False
        self.assertEqual(self.reuse()["sha256"], self.proof["sha256"])

    def test_tampered_bundle_is_rejected_before_copy(self):
        self.bundle.write_bytes(b"tampered generated bundle")
        with self.assertRaises(mutation.BindingError):
            self.reuse()
        self.assertFalse(self.isolated.exists())

    def test_changed_source_lock_or_tools_are_rejected_before_copy(self):
        for index in range(6):
            values = ["tree", "tree", "lock", "lock", "v26.6.0", "11.18.0"]
            values[index] = "changed"
            with self.subTest(index=index), self.assertRaises(mutation.BindingError):
                self.reuse(values)
            self.assertFalse(self.isolated.exists())

    def test_unproven_or_failed_build_is_rejected(self):
        for field in ("official_build_script", "scripts_ignored_on_install"):
            self.proof[field] = False
            with self.assertRaises(mutation.BindingError):
                self.reuse()
            self.proof[field] = True
        for stage in self.receipt["stages"]:
            stage["exit"] = 1
            with self.assertRaises(mutation.BindingError):
                self.reuse()
            stage["exit"] = 0
        self.assertFalse(self.isolated.exists())


class CleanupTests(unittest.TestCase):
    def inspect_denied_signal(self, stdout):
        child = unittest.mock.Mock(pid=987654)
        child.wait.return_value = 0
        row = {}
        with patch.object(mutation.os, "killpg", side_effect=PermissionError(1, "denied")), \
             patch.object(mutation.subprocess, "run", return_value=unittest.mock.Mock(stdout=stdout)):
            try:
                mutation.stop_owned_group(child, row)
                error = None
            except Exception as exc:
                error = exc
        return row, error

    def test_permission_error_is_absence_only_after_independent_group_check(self):
        row, error = self.inspect_denied_signal("123 456\n")
        self.assertIsNone(error)
        self.assertEqual(row["denied_signal"]["members"], [])
        self.assertIn("ps verified", row["cleanup"])

    def test_permission_error_with_present_group_cannot_claim_cleanup(self):
        row, error = self.inspect_denied_signal("987654 987654\n987655 987654\n")
        self.assertIsInstance(error, mutation.CleanupError)
        self.assertEqual(row["denied_signal"]["members"], [987654, 987655])
        self.assertNotIn("cleanup", row)

    def test_unreadable_group_snapshot_cannot_claim_absence(self):
        row, error = self.inspect_denied_signal("not numeric process metadata\n")
        self.assertIsInstance(error, mutation.CleanupError)
        self.assertNotIn("cleanup", row)


@unittest.skipUnless(os.name == "posix", "mutation runner requires POSIX process-group cleanup")
class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / "operator"
        self.root.mkdir()
        self.checkout = self.base / "cargo-checkout"
        self.canonical = self.checkout / "native/web-stack/Cargo.toml"
        self.canonical.parent.mkdir(parents=True)
        self.manifest_text = '[dependencies]\nctox-pdf-parse = { path = "../pdf-parse", optional = true }\n'
        self.canonical.write_text(self.manifest_text)
        self.original = (mutation.MARKER + "\n})();").encode()
        assets = self.canonical.parent / "assets"
        assets.mkdir()
        (assets / "stealth_init.js").write_bytes(self.original)
        self.binding = {"manifest": str(self.canonical), "revision": "b" * 40,
                        "source": "https://github.com/metric-space-ai/workjet#" + "b" * 40,
                        "target_directory": str(self.base / "task-target")}
        self.probes = []
        self.commands = []
        self.asset = None
        self.sandbox = None

    def run_fake(self, negative="failed", wrong_package=False, changed_root=False, final_error=False, wrong_pdf=False, timeout_stage=None, cleanup_denied=False):
        def copy(checkout, revision, destination, deadline):
            destination.mkdir()
            if destination.name == "ctox":
                (destination / "Cargo.toml").write_text("root manifest")
                (destination / "Cargo.lock").write_text("isolated lock")
            else:
                manifest = destination / "native/web-stack/Cargo.toml"
                manifest.parent.mkdir(parents=True)
                manifest.write_text(self.manifest_text)
                self.asset = manifest.parent / "assets/stealth_init.js"
                self.asset.parent.mkdir()
                self.asset.write_bytes(self.original)
            self.sandbox = destination.parent
        def popen(command, **kwargs):
            self.commands.append(command)
            label = Path(kwargs["stdout"].name).stem
            self.current_label = label
            process = unittest.mock.Mock(pid=987654)
            process.wait.return_value = 0
            root = Path(kwargs["cwd"])
            if label == "resolve-isolated":
                metadata = {"packages": [
                    {"id": "root", "name": "ctox", "manifest_path": str(root / "Cargo.toml")},
                    {"id": "copied", "name": "ctox-web-stack", "source": None,
                     "manifest_path": str(self.asset.parent.parent / "Cargo.toml") if not wrong_package else str(self.canonical)},
                    {"id": "git-pdf", "name": "ctox-pdf-parse", "source": None if wrong_pdf else mutation.git_pdf_source(self.binding)[1],
                     "manifest_path": str(self.canonical.parent.parent / "pdf-parse/Cargo.toml")}],
                    "resolve": {"nodes": [{"id": "root", "deps": [{"name": "ctox_web_stack", "pkg": "copied"}]},
                                           {"id": "copied", "deps": [{"name": "ctox_pdf_parse", "pkg": "git-pdf"}]}]}}
                json.dump(metadata, kwargs["stdout"])
            elif label.startswith("build-"):
                executable = Path(kwargs["env"]["CARGO_TARGET_DIR"]) / "debug/ctox"
                executable.parent.mkdir(parents=True, exist_ok=True)
                executable.write_bytes(b"compiled:" + self.asset.read_bytes())
                for artifact in ({"reason": "compiler-artifact", "package_id": "copied"},
                                 {"reason": "compiler-artifact", "package_id": "root", "target": {"name": "ctox"}, "executable": str(executable)}):
                    kwargs["stdout"].write(json.dumps(artifact) + "\n")
            elif label.endswith("positive") or label == "mutated-negative":
                self.probes.append(label)
                if label == "mutated-negative" and negative == "interrupted":
                    process.wait.side_effect = [KeyboardInterrupt("cancelled"), 0]
                elif label == "mutated-negative" and negative == "malformed":
                    kwargs["stdout"].write('{"error":"network"}')
                    process.wait.return_value = 1
                else:
                    failed = label == "mutated-negative" and negative == "failed"
                    json.dump({"probes": [{"failed_count": int(failed), "failed_tests": ["webdriver"] if failed else []}]}, kwargs["stdout"])
                    process.wait.return_value = int(failed)
            if label == timeout_stage:
                process.wait.side_effect = [subprocess.TimeoutExpired(command, 1200), 0]
            self.assertEqual(kwargs["env"]["CARGO_BUILD_JOBS"], "2")
            self.assertEqual(kwargs["env"]["CTOX_STATE_ROOT"], str(root / "runtime"))
            self.assertEqual(kwargs["env"]["CARGO_TARGET_DIR"], self.binding["target_directory"])
            return process
        def killpg(pid, signum):
            if cleanup_denied and self.current_label == timeout_stage:
                raise PermissionError(1, "denied")
            raise ProcessLookupError
        states = [{"head": "a" * 40}, {"head": "c" * 40 if changed_root else "a" * 40}]
        old_handler = signal.getsignal(signal.SIGTERM)
        with patch.object(mutation, "source_state", side_effect=states), \
             patch.object(mutation, "capture", side_effect=["", str(self.checkout)]), \
             patch.object(mutation, "resolve", side_effect=mutation.BindingError("source verification deadline reached") if final_error else None,
                          return_value=self.binding), \
             patch.object(mutation, "copy_revision", side_effect=copy), \
             patch.object(mutation, "reuse_sidecar_bundle", return_value={"sha256": "proved"}), \
             patch.object(mutation.tempfile, "gettempdir", return_value=str(self.base)), \
             patch.object(mutation.sys, "platform", "linux"), \
             patch.object(mutation.subprocess, "Popen", side_effect=popen), \
             patch.object(mutation.os, "killpg", side_effect=killpg), \
             patch.object(mutation.subprocess, "run", return_value=unittest.mock.Mock(stdout="987654 987654\n")), \
             patch.dict(os.environ, {"CARGO_BUILD_JOBS": "2", "RUST_TEST_THREADS": "2"}), \
             contextlib.redirect_stdout(io.StringIO()):
            try:
                result = mutation.run_mutation(self.root, self.binding, sidecar_build_receipt=self.base / "build.json")
                error = None
            except BaseException as exc:
                result, error = None, exc
        self.assertEqual(signal.getsignal(signal.SIGTERM), old_handler)
        self.assertEqual(self.asset.read_bytes(), self.original)
        self.assertEqual((self.asset.parent.parent / "Cargo.toml").read_text(), self.manifest_text)
        self.assertEqual(self.canonical.read_text(), self.manifest_text)
        self.assertEqual((self.canonical.parent / "assets/stealth_init.js").read_bytes(), self.original)
        receipt = json.loads((self.sandbox / "result.json").read_text())
        return result, error, receipt

    def test_timeout_remains_primary_when_cleanup_signal_is_denied(self):
        _, error, receipt = self.run_fake(timeout_stage="build-initial", cleanup_denied=True)
        self.assertIsInstance(error, subprocess.TimeoutExpired)
        self.assertTrue(receipt["cleanup_failed"])
        self.assertFalse(receipt["passed"])
        self.assertIn("still present", receipt["steps"][-1]["cleanup_error"])
        self.assertEqual(self.probes, [])
        self.assertEqual(receipt["limits"]["seconds"], 1800)
        self.assertEqual(receipt["limits"]["build_seconds"], 1200)

    def test_unresolved_negative_cleanup_restores_without_starting_control(self):
        _, error, receipt = self.run_fake(timeout_stage="mutated-negative", cleanup_denied=True)
        self.assertIsInstance(error, subprocess.TimeoutExpired)
        self.assertTrue(receipt["asset_restored"])
        self.assertTrue(receipt["manifest_restored"])
        self.assertFalse(receipt["passed"])
        self.assertNotIn("restored-positive", self.probes)
        self.assertNotIn("build-restored", receipt)

    def test_full_cycle_isolates_source_and_checks_restored_control(self):
        result, error, receipt = self.run_fake()
        self.assertIsNone(error)
        self.assertEqual(result, 0)
        self.assertTrue(receipt["passed"])
        self.assertTrue(receipt["original_binding_unchanged"])
        self.assertTrue(receipt["asset_restored"])
        self.assertTrue(receipt["manifest_restored"])
        self.assertEqual(self.probes, ["initial-positive", "mutated-negative", "restored-positive"])
        self.assertNotEqual(receipt["build-mutated"]["sha256"], receipt["build-initial"]["sha256"])
        self.assertEqual(receipt["build-restored"]["sha256"], receipt["build-initial"]["sha256"])
        self.assertTrue(any("--config" in command and "--locked" in command for command in self.commands))

    def test_malformed_negative_still_runs_restored_positive_and_fails(self):
        result, error, receipt = self.run_fake(negative="malformed")
        self.assertIsInstance(error, mutation.BindingError)
        self.assertFalse(receipt["passed"])
        self.assertIn("restored-positive", self.probes)

    def test_unexpected_mutation_pass_still_runs_control_and_fails(self):
        _, error, receipt = self.run_fake(negative="passed")
        self.assertIsInstance(error, mutation.BindingError)
        self.assertFalse(receipt["passed"])
        self.assertIn("restored-positive", self.probes)

    def test_cancel_restores_asset_without_claiming_a_control(self):
        _, error, receipt = self.run_fake(negative="interrupted")
        self.assertIsInstance(error, KeyboardInterrupt)
        self.assertTrue(receipt["asset_restored"])
        self.assertFalse(receipt["passed"])
        self.assertNotIn("restored-positive", self.probes)

    def test_wrong_isolated_dependency_fails_before_build_and_probe(self):
        _, error, receipt = self.run_fake(wrong_package=True)
        self.assertIsInstance(error, mutation.BindingError)
        self.assertFalse(receipt["passed"])
        self.assertEqual(self.probes, [])

    def test_wrong_pdf_source_fails_before_build_or_probe(self):
        _, error, receipt = self.run_fake(wrong_pdf=True)
        self.assertIsInstance(error, mutation.BindingError)
        self.assertFalse(receipt["passed"])
        self.assertTrue(receipt["manifest_restored"])
        self.assertEqual(self.probes, [])
        self.assertFalse(any('build' in command for command in self.commands))

    def test_unavailable_final_verification_cannot_claim_a_pass(self):
        result, error, receipt = self.run_fake(final_error=True)
        self.assertIsNone(error)
        self.assertEqual(result, 1)
        self.assertFalse(receipt["passed"])
        self.assertTrue(receipt["asset_restored"])
        self.assertIn("deadline reached", receipt["final_validation_error"])

    def test_changed_operator_source_invalidates_otherwise_passing_cycle(self):
        result, error, receipt = self.run_fake(changed_root=True)
        self.assertIsNone(error)
        self.assertEqual(result, 1)
        self.assertFalse(receipt["passed"])
        self.assertFalse(receipt["original_root_unchanged"])


if __name__ == "__main__":
    unittest.main()
