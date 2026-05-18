#!/usr/bin/env python3
"""Create a timestamped backup of an installed skill directory."""

from __future__ import annotations

import argparse
from datetime import datetime
from pathlib import Path
import shutil
import sys


def create_backup(skill_dir: Path) -> Path:
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    backup_dir = skill_dir.parent / f"{skill_dir.name}-backup-{timestamp}"
    shutil.copytree(skill_dir, backup_dir)
    return backup_dir


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Create a timestamped backup of a skill directory.")
    parser.add_argument("skill_dir", help="Path to the installed skill directory")
    args = parser.parse_args(argv)

    skill_dir = Path(args.skill_dir).resolve()
    if not skill_dir.exists():
        print(f"[ERROR] Skill directory not found: {skill_dir}", file=sys.stderr)
        return 1
    if not skill_dir.is_dir():
        print(f"[ERROR] Path is not a directory: {skill_dir}", file=sys.stderr)
        return 1

    backup_dir = create_backup(skill_dir)
    print(backup_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
