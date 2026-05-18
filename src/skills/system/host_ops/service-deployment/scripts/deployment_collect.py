#!/usr/bin/env python3
import argparse
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


COLLECTORS = {
    "host_identity": [
        ["hostnamectl"],
        ["uname", "-a"],
    ],
    "package_managers": [
        ["bash", "-lc", "command -v apt || true"],
        ["bash", "-lc", "command -v snap || true"],
        ["bash", "-lc", "command -v docker || true"],
        ["bash", "-lc", "command -v podman || true"],
        ["bash", "-lc", "command -v systemctl || true"],
    ],
    "ports": [
        ["ss", "-ltn"],
    ],
    "service_presence": [
        ["bash", "-lc", "snap list 2>/dev/null || true"],
        ["bash", "-lc", "docker ps --format '{{.Names}} {{.Ports}}' 2>/dev/null || true"],
        ["bash", "-lc", "systemctl list-unit-files --type=service --no-pager 2>/dev/null || true"],
    ],
}


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def run_argv(argv: list[str]) -> dict:
    started_at = now_iso()
    proc = subprocess.run(argv, capture_output=True, text=True)
    finished_at = now_iso()
    return {
        "tool": Path(argv[0]).name,
        "argv": argv,
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "exit_code": proc.returncode,
        "started_at": started_at,
        "finished_at": finished_at,
    }


def list_collectors() -> int:
    print(json.dumps(sorted(COLLECTORS), indent=2))
    return 0


def run_collector(name: str) -> int:
    if name not in COLLECTORS:
        raise SystemExit(f"unknown collector: {name}")
    payload = {
        "collector": name,
        "captured_at": now_iso(),
        "captures": [run_argv(argv) for argv in COLLECTORS[name]],
    }
    print(json.dumps(payload, indent=2))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Raw preflight collectors for service deployment.")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list-collectors")
    run_parser = sub.add_parser("run-collector")
    run_parser.add_argument("--collector", required=True)
    args = parser.parse_args()
    if args.command == "list-collectors":
        return list_collectors()
    return run_collector(args.collector)


if __name__ == "__main__":
    sys.exit(main())
