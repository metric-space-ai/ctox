#!/usr/bin/env python3
import argparse
import os
import subprocess
import sys
from pathlib import Path


def load_env_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        return values
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        values[key.strip()] = value.strip()
    return values


def main() -> int:
    parser = argparse.ArgumentParser(description="Visible sudo helper for CTOX local privileged actions.")
    parser.add_argument("--root", default=".")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not args.command:
        raise SystemExit("missing command")
    try:
        probe = subprocess.run(
            ["sudo", "-n", "true"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if probe.returncode == 0:
            proc = subprocess.run(["sudo", *args.command], text=True)
            return proc.returncode
    except Exception:
        pass
    root = Path(args.root).resolve()
    env_path = root / "runtime" / "secrets" / "ctox-sudo.env"
    values = load_env_file(env_path)
    password = values.get("CTOX_SUDO_PASSWORD") or os.environ.get("CTOX_SUDO_PASSWORD")
    if not password:
        raise SystemExit(f"missing sudo secret reference: {env_path}")
    proc = subprocess.run(
        ["sudo", "-S", "-p", "", *args.command],
        input=password + "\n",
        text=True,
    )
    return proc.returncode


if __name__ == "__main__":
    raise SystemExit(main())
