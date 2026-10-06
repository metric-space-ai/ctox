"""Runtime payload provenance and isolated-root preservation regressions."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "src/scripts/transfer-runtime-bundle.py"
SPEC = importlib.util.spec_from_file_location("transfer_runtime_bundle", SCRIPT)
bundle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bundle)


class RuntimeBundleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.repository = self.base / "source"
        self.repository.mkdir()
        self.git("init", "-q")
        self.git("config", "user.name", "Transfer fixture")
        self.git("config", "user.email", "transfer@example.invalid")
        for path in bundle.REQUIRED:
            target = self.repository / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((path + "\n").encode())
        # Neither another tracked source path nor ignored/untracked state belongs in the payload.
        (self.repository / "private-state").write_text("fixture state")
        self.git("add", ".")
        self.git("commit", "-qm", "runtime source")
        self.revision = self.git("rev-parse", "HEAD").strip()
        self.output = self.base / "bundle"
        self.output.mkdir()
        (self.output / "runtime").mkdir()
        (self.output / "runtime/identity").write_text("retained fixture identity")

    def git(self, *arguments):
        return subprocess.check_output(["git", "-C", str(self.repository), *arguments], text=True)

    def test_exact_committed_payload_preserves_existing_identity(self):
        path = "src/apps/business-os/index.html"
        (self.repository / path).write_text("uncommitted replacement")
        (self.repository / "src/apps/business-os/local-secret").write_text("untracked fixture")
        result = bundle.materialize(self.repository, self.revision, self.output)
        manifest = json.loads(Path(result["manifest"]).read_text())
        self.assertEqual(manifest["source_commit"], self.revision)
        self.assertEqual(manifest["file_count"], len(bundle.REQUIRED))
        for entry in manifest["files"]:
            actual = (self.output / entry["path"]).read_bytes()
            expected = (entry["path"] + "\n").encode()
            self.assertEqual(actual, expected)
            self.assertEqual(entry["sha256"], hashlib.sha256(actual).hexdigest())
            self.assertEqual(entry["size"], len(actual))
        self.assertFalse((self.output / "private-state").exists())
        self.assertFalse((self.output / "src/apps/business-os/local-secret").exists())
        self.assertEqual((self.output / "runtime/identity").read_text(), "retained fixture identity")
        before = (self.output / path).read_bytes()
        with self.assertRaisesRegex(ValueError, "no src entry"):
            bundle.materialize(self.repository, self.revision, self.output)
        self.assertEqual((self.output / path).read_bytes(), before)

    def test_existing_dangling_src_and_linked_asset_are_rejected(self):
        (self.output / "src").symlink_to(self.base / "missing")
        with self.assertRaisesRegex(ValueError, "no src entry"):
            bundle.materialize(self.repository, self.revision, self.output)
        (self.output / "src").unlink()
        (self.repository / "src/apps/business-os/link").symlink_to("../../private-state")
        self.git("add", ".")
        self.git("commit", "-qm", "linked source asset")
        with self.assertRaisesRegex(ValueError, "regular tracked files"):
            bundle.materialize(self.repository, self.git("rev-parse", "HEAD").strip(), self.output)
        self.assertFalse((self.output / "src").exists())

    def test_source_git_metadata_and_mutable_revision_are_rejected(self):
        for destination in (self.repository, self.repository / ".git"):
            with self.assertRaisesRegex(ValueError, "outside"):
                bundle.materialize(self.repository, self.revision, destination)
        with self.assertRaisesRegex(ValueError, "immutable commit"):
            bundle.materialize(self.repository, "HEAD", self.output)
        self.assertFalse((self.output / "src").exists())


if __name__ == "__main__":
    unittest.main()
