#!/usr/bin/env python3
from __future__ import annotations

import argparse
import fnmatch
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


EXCLUDED_DIRECTORIES = {
    ".git",
    ".next",
    ".turbo",
    ".vercel",
    "coverage",
    "dist",
    "node_modules",
}

EXCLUDED_FILES = {
    ".DS_Store",
    ".env",
    "pnpm-debug.log",
}

EXCLUDED_PATTERNS = {
    ".env.*",
    "*.tsbuildinfo",
    "*.log",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Install the CTOX Business Basic Stack as a separate customer-owned repo."
    )
    parser.add_argument(
        "--target",
        required=True,
        type=Path,
        help="Directory that will receive the generated business repository.",
    )
    parser.add_argument(
        "--ctox-repo",
        type=Path,
        default=None,
        help="CTOX core repository root. Defaults to auto-detection from this script.",
    )
    parser.add_argument(
        "--template",
        default="templates/business-basic",
        help="Template path relative to the CTOX repo root.",
    )
    parser.add_argument(
        "--init-git",
        action="store_true",
        help="Initialize a Git repository and create the first commit.",
    )
    parser.add_argument(
        "--no-copy-env",
        action="store_true",
        help="Do not copy .env.example to .env after installation.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would be installed without writing files.",
    )
    return parser.parse_args()


def find_ctox_repo(start: Path) -> Path:
    for candidate in [start, *start.parents]:
        if (candidate / "templates/business-basic/ctox-business.json").is_file():
            return candidate
    raise SystemExit(
        "Could not locate CTOX repo root. Pass --ctox-repo explicitly."
    )


def resolve_ctox_repo(explicit_repo: Path | None) -> Path:
    if explicit_repo is not None:
        repo = explicit_repo.expanduser().resolve()
        if not (repo / "templates/business-basic/ctox-business.json").is_file():
            raise SystemExit(f"Invalid CTOX repo root: {repo}")
        return repo
    return find_ctox_repo(Path(__file__).resolve())


def is_excluded(relative_path: Path) -> bool:
    if any(part in EXCLUDED_DIRECTORIES for part in relative_path.parts):
        return True

    name = relative_path.name
    if name in EXCLUDED_FILES:
        return True

    if name == ".env.example":
        return False

    return any(fnmatch.fnmatch(name, pattern) for pattern in EXCLUDED_PATTERNS)


def validate_target(repo_root: Path, template_root: Path, target: Path) -> Path:
    resolved = target.expanduser().resolve()

    if resolved == repo_root:
        raise SystemExit("Refusing to install into the CTOX core repo root.")
    if resolved == template_root:
        raise SystemExit("Refusing to install into the source template directory.")
    if template_root in resolved.parents:
        raise SystemExit("Refusing to install inside the source template directory.")

    if resolved.exists() and any(resolved.iterdir()):
        raise SystemExit(
            f"Target directory is not empty: {resolved}\n"
            "Choose an empty directory so customer customizations are never overwritten."
        )

    return resolved


def collect_files(template_root: Path) -> list[tuple[Path, Path]]:
    files: list[tuple[Path, Path]] = []
    for source in template_root.rglob("*"):
        relative = source.relative_to(template_root)
        if is_excluded(relative):
            continue
        if source.is_file():
            files.append((source, relative))
    return files


def copy_files(files: list[tuple[Path, Path]], target: Path) -> None:
    target.mkdir(parents=True, exist_ok=True)
    for source, relative in files:
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)


def write_manifest(repo_root: Path, template_root: Path, target: Path) -> None:
    template_meta = json.loads((template_root / "ctox-business.json").read_text())
    manifest = {
        "schemaVersion": 1,
        "installedAt": datetime.now(timezone.utc).isoformat(),
        "templateId": template_meta.get("templateId", "ctox-business-basic"),
        "displayName": template_meta.get("displayName", "CTOX Business Basic"),
        "source": {
            "ctoxRepo": str(repo_root),
            "templatePath": str(template_root.relative_to(repo_root)),
        },
        "repositoryOwnership": "customer-owned",
        "coreUpgradePolicy": "never_overwrite_generated_repo",
        "customizationPolicy": (
            "Customize this generated repository. CTOX core upgrades must only propose "
            "normal Git diffs here and never replace files in place."
        ),
    }
    (target / ".ctox-business-install.json").write_text(
        json.dumps(manifest, indent=2) + "\n"
    )


def copy_env_example(target: Path) -> None:
    env_example = target / ".env.example"
    env_file = target / ".env"
    if env_example.is_file() and not env_file.exists():
        shutil.copy2(env_example, env_file)


def run_git_init(target: Path) -> None:
    commands = [
        ["git", "init"],
        ["git", "add", "."],
        [
            "git",
            "-c",
            "user.name=CTOX Business Stack Installer",
            "-c",
            "user.email=ctox-business-stack@example.invalid",
            "commit",
            "-m",
            "Initialize CTOX business basic stack",
        ],
    ]
    for command in commands:
        subprocess.run(command, cwd=target, check=True)


def main() -> int:
    args = parse_args()
    repo_root = resolve_ctox_repo(args.ctox_repo)
    template_root = (repo_root / args.template).resolve()
    if not (template_root / "ctox-business.json").is_file():
        raise SystemExit(f"Template is missing ctox-business.json: {template_root}")

    target = validate_target(repo_root, template_root, args.target)
    files = collect_files(template_root)

    if args.dry_run:
        print(f"CTOX repo: {repo_root}")
        print(f"Template: {template_root}")
        print(f"Target: {target}")
        print(f"Files to copy: {len(files)}")
        print("Would write .ctox-business-install.json")
        if not args.no_copy_env:
            print("Would copy .env.example to .env when available")
        if args.init_git:
            print("Would initialize git and create initial commit")
        return 0

    copy_files(files, target)
    write_manifest(repo_root, template_root, target)

    if not args.no_copy_env:
        copy_env_example(target)

    if args.init_git:
        run_git_init(target)

    print(f"Installed CTOX Business Basic Stack in {target}")
    print("Ownership: customer-owned generated repository")
    print("Core upgrades: never overwrite this repo in place")
    return 0


if __name__ == "__main__":
    sys.exit(main())
