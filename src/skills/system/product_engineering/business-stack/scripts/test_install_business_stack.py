#!/usr/bin/env python3
from __future__ import annotations

import json
import importlib.util
import subprocess
import sys
import tempfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("install_business_stack.py")
CTOX_REPO = SCRIPT.parents[5]


def main() -> int:
    installer = load_installer()
    assert_exclusion_rules(installer)

    with tempfile.TemporaryDirectory(prefix="ctox-business-stack-install-") as root:
        temp_root = Path(root)
        dry_run_target = temp_root / "dry-run"
        install_target = temp_root / "installed"
        non_empty_target = temp_root / "non-empty"
        git_target = temp_root / "git-installed"

        dry_run = run_installer("--target", dry_run_target, "--ctox-repo", CTOX_REPO, "--dry-run")
        assert_ok(dry_run.returncode == 0, dry_run.stderr)
        assert_ok(not dry_run_target.exists(), "dry-run unexpectedly created the target directory")
        assert_ok("Files to copy:" in dry_run.stdout, "dry-run did not report copied files")

        install = run_installer("--target", install_target, "--ctox-repo", CTOX_REPO)
        assert_ok(install.returncode == 0, install.stderr)
        assert_install_shape(install_target)

        non_empty_target.mkdir()
        (non_empty_target / "keep.txt").write_text("existing customer code\n")
        non_empty = run_installer("--target", non_empty_target, "--ctox-repo", CTOX_REPO)
        assert_ok(non_empty.returncode != 0, "non-empty target should have been rejected")
        assert_ok("not empty" in non_empty.stderr, "non-empty target error should explain the risk")

        git_install = run_installer("--target", git_target, "--ctox-repo", CTOX_REPO, "--init-git")
        assert_ok(git_install.returncode == 0, git_install.stderr)
        assert_install_shape(git_target)
        assert_ok((git_target / ".git").is_dir(), "--init-git did not create a git repository")
        ignored_files = subprocess.run(
            [
                "git",
                "check-ignore",
                ".env",
                "node_modules/.probe",
                "apps/web/.next/cache",
                "apps/web/next-env.d.ts",
            ],
            cwd=git_target,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.splitlines()
        assert_ok(".env" in ignored_files, ".env should be git-ignored")
        assert_ok("node_modules/.probe" in ignored_files, "node_modules should be git-ignored")
        assert_ok("apps/web/.next/cache" in ignored_files, ".next should be git-ignored")
        assert_ok("apps/web/next-env.d.ts" in ignored_files, "next-env.d.ts should be git-ignored")
        commit_count = subprocess.run(
            ["git", "rev-list", "--count", "HEAD"],
            cwd=git_target,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        assert_ok(commit_count == "1", f"--init-git expected one initial commit, got {commit_count}")

    print("Business stack installer smoke passed")
    return 0


def load_installer():
    spec = importlib.util.spec_from_file_location("install_business_stack", SCRIPT)
    assert_ok(spec is not None and spec.loader is not None, "could not load installer module")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def assert_exclusion_rules(installer) -> None:
    assert_ok(installer.is_excluded(Path(".env")), ".env should be excluded")
    assert_ok(installer.is_excluded(Path(".env.local")), ".env.local should be excluded")
    assert_ok(installer.is_excluded(Path("apps/web/.env.production")), ".env.production should be excluded")
    assert_ok(not installer.is_excluded(Path(".env.example")), ".env.example should be copied")
    assert_ok(installer.is_excluded(Path("apps/web/.next")), ".next should be excluded")
    assert_ok(installer.is_excluded(Path("apps/web/tsconfig.tsbuildinfo")), "tsbuildinfo should be excluded")


def run_installer(*args: object) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *[str(arg) for arg in args]],
        capture_output=True,
        text=True,
    )


def assert_install_shape(target: Path) -> None:
    assert_ok((target / "package.json").is_file(), "package.json missing")
    assert_ok((target / "README.md").is_file(), "README.md missing")
    assert_ok((target / ".env.example").is_file(), ".env.example missing")
    assert_ok((target / ".env").is_file(), ".env was not copied from .env.example")
    assert_ok((target / "apps/web/next-env.d.ts").is_file(), "next-env.d.ts missing")
    assert_ok((target / ".ctox-business-install.json").is_file(), "install manifest missing")
    assert_ok(not (target / "node_modules").exists(), "node_modules should not be copied")
    assert_ok(not (target / "apps/web/.next").exists(), ".next should not be copied")
    assert_ok(not any(target.rglob("*.tsbuildinfo")), "tsbuildinfo files should not be copied")

    manifest = json.loads((target / ".ctox-business-install.json").read_text())
    assert_ok(manifest["repositoryOwnership"] == "customer-owned", "manifest ownership is wrong")
    assert_ok(
        manifest["coreUpgradePolicy"] == "never_overwrite_generated_repo",
        "manifest core upgrade policy is wrong",
    )


def assert_ok(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


if __name__ == "__main__":
    raise SystemExit(main())
