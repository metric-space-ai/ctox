#!/usr/bin/env python3
from __future__ import annotations

import json
import importlib.util
import subprocess
import sys
import tempfile
from pathlib import Path


SCRIPT = Path(__file__).with_name("install_business_stack.py")
CTOX_REPO = next(
    parent
    for parent in [SCRIPT, *SCRIPT.parents]
    if (parent / "templates/business-basic/ctox-business.json").is_file()
)


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
        if (git_target / ".gitignore").is_file():
            ignored_files = subprocess.run(
                [
                    "git",
                    "check-ignore",
                    ".env",
                    "node_modules/.probe",
                    "dist/app.js",
                    "output/install.log",
                ],
                cwd=git_target,
                check=True,
                capture_output=True,
                text=True,
            ).stdout.splitlines()
            assert_ok(".env" in ignored_files, ".env should be git-ignored")
            assert_ok("node_modules/.probe" in ignored_files, "node_modules should be git-ignored")
            assert_ok("dist/app.js" in ignored_files, "dist should be git-ignored")
            assert_ok("output/install.log" in ignored_files, "output should be git-ignored")
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
    assert_ok(installer.is_excluded(Path("runtime/.env.production")), ".env.production should be excluded")
    assert_ok(not installer.is_excluded(Path(".env.example")), ".env.example should be copied")
    assert_ok(installer.is_excluded(Path(".next/cache")), ".next should be excluded")
    assert_ok(installer.is_excluded(Path("runtime/tsconfig.tsbuildinfo")), "tsbuildinfo should be excluded")
    assert_ok(
        installer.is_excluded(Path("apps/web/app/page.tsx")),
        "legacy Next.js business app must not be installed",
    )
    assert_ok(
        installer.is_excluded(Path("public-website-repo/app/page.tsx")),
        "legacy Next.js public website bridge must not be installed",
    )
    assert_ok(
        not installer.is_excluded(Path("modules/ctox/module.json")),
        "system Business OS modules must remain installable",
    )
    assert_ok(
        not installer.is_excluded(Path("src/apps/business-os/modules/app-store/module.json")),
        "system Business OS apps must remain installable",
    )
    assert_ok(
        installer.is_excluded(Path("modules/notizen/module.json")),
        "non-system modules must come from the app store, not installation",
    )
    assert_ok(
        installer.is_excluded(Path("installed-modules/matching/module.json")),
        "installed modules must not be bundled by installation",
    )
    assert_ok(
        installer.is_excluded(Path("src/apps/business-os/modules/notes/module.json")),
        "non-system native apps must come from the app store",
    )


def run_installer(*args: object) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), *[str(arg) for arg in args]],
        capture_output=True,
        text=True,
    )


def assert_install_shape(target: Path) -> None:
    assert_ok((target / "ctox-business.json").is_file(), "ctox-business.json missing")
    assert_ok((target / ".ctox-business-install.json").is_file(), "install manifest missing")
    assert_ok(not (target / ".env").exists(), ".env should not be created without .env.example")
    assert_ok(not (target / "node_modules").exists(), "node_modules should not be copied")
    assert_ok(not (target / "apps/web").exists(), "legacy Next.js business app should not be copied")
    assert_ok(
        not (target / "public-website-repo").exists(),
        "legacy Next.js public website bridge should not be copied",
    )
    assert_ok(not any(target.rglob("*.tsbuildinfo")), "tsbuildinfo files should not be copied")

    manifest = json.loads((target / ".ctox-business-install.json").read_text())
    assert_ok(manifest["repositoryOwnership"] == "customer-owned", "manifest ownership is wrong")
    assert_ok(
        manifest["coreUpgradePolicy"] == "never_overwrite_generated_repo",
        "manifest core upgrade policy is wrong",
    )
    assert_ok(
        manifest["appInstallPolicy"]["nonSystemApps"] == "app-store-only",
        "non-system apps must be app-store-only",
    )
    assert_ok(
        manifest["appInstallPolicy"]["runtime"] == "rxdb-business-os",
        "Business OS runtime should be RxDB",
    )
    assert_ok(
        manifest["appInstallPolicy"]["legacyNextBusinessApp"] == "excluded",
        "legacy Next.js app must stay excluded",
    )
    assert_ok(
        "app-store" in manifest["appInstallPolicy"]["systemAppsInstalled"],
        "system app allowlist missing app-store",
    )


def assert_ok(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


if __name__ == "__main__":
    raise SystemExit(main())
