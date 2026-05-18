#!/usr/bin/env python3
"""Validate a skill and regenerate agents/openai.yaml."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys

from generate_openai_yaml import read_frontmatter_name
from generate_openai_yaml import write_openai_yaml
from quick_validate import validate_skill


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        description="Validate a skill and refresh agents/openai.yaml.",
    )
    parser.add_argument("skill_dir", help="Path to the skill directory")
    parser.add_argument(
        "--name",
        help="Skill name override (defaults to SKILL.md frontmatter)",
    )
    parser.add_argument(
        "--interface",
        action="append",
        default=[],
        help="Interface override in key=value format (repeatable)",
    )
    args = parser.parse_args(argv)

    skill_dir = Path(args.skill_dir).resolve()
    valid, message = validate_skill(skill_dir)
    if not valid:
        print(message, file=sys.stderr)
        return 1

    skill_name = args.name or read_frontmatter_name(skill_dir)
    if not skill_name:
        return 1

    result = write_openai_yaml(skill_dir, skill_name, args.interface)
    if result is None:
        return 1

    print(message)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
