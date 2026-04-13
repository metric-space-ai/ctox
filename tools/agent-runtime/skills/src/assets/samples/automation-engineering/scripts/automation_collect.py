#!/usr/bin/env python3
import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def stable_run_id(scope: dict[str, Any], captured_at: str) -> str:
    payload = json.dumps({"scope": scope, "captured_at": captured_at}, sort_keys=True)
    digest = hashlib.sha256(payload.encode("utf-8")).hexdigest()[:16]
    return f"run-{digest}"


def command_available(name: str) -> bool:
    return shutil.which(name) is not None


def collector_specs(repo_root: Optional[str]) -> Dict[str, List[Dict[str, Any]]]:
    specs: Dict[str, List[Dict[str, Any]]] = {
        "ctox_state": [],
        "repo_scripts": [],
    }
    if command_available("ctox"):
        specs["ctox_state"].extend(
            [
                {"tool": "ctox", "argv": ["ctox", "queue", "list", "--limit", "50"]},
                {"tool": "ctox", "argv": ["ctox", "schedule", "list"]},
                {"tool": "ctox", "argv": ["ctox", "plan", "list"]},
            ]
        )
    if repo_root:
        specs["repo_scripts"].extend(
            [
                {"tool": "rg", "argv": ["rg", "--files", repo_root]},
                {"tool": "rg", "argv": ["rg", "-n", "ctox schedule add|ctox queue add|cron|systemctl", repo_root]},
            ]
        )
    return specs


def run_command(spec: Dict[str, Any]) -> Dict[str, Any]:
    argv = list(spec["argv"])
    started_at = now_iso()
    monotonic_start = time.monotonic()
    resolved = shutil.which(argv[0])
    if resolved is None:
        return {"tool": spec["tool"], "argv": argv, "started_at": started_at, "finished_at": now_iso(), "duration_ms": 0, "exit_code": None, "available": False, "stdout": "", "stderr": f"command not found: {argv[0]}"}
    completed = subprocess.run(argv, capture_output=True, text=True, check=False)
    return {"tool": spec["tool"], "argv": argv, "started_at": started_at, "finished_at": now_iso(), "duration_ms": int((time.monotonic() - monotonic_start) * 1000), "exit_code": completed.returncode, "available": True, "stdout": completed.stdout, "stderr": completed.stderr}


def build_capture(collector: str, repo_root: Optional[str], commands: List[Dict[str, Any]], run_id: Optional[str] = None) -> Dict[str, Any]:
    captured_at = now_iso()
    scope = {"repo_root": repo_root}
    return {"model_version": 1, "skill_key": "automation_engineering", "run_id": run_id or stable_run_id(scope, captured_at), "captured_at": captured_at, "collector": collector, "scope": scope, "captures": [run_command(spec) for spec in commands]}


def main() -> int:
    parser = argparse.ArgumentParser(description="Run automation-engineering collectors and return raw JSON captures.")
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name in ("list-collectors", "run", "run-all"):
        sub = subparsers.add_parser(name)
        if name == "run":
            sub.add_argument("--collector", required=True)
        sub.add_argument("--repo-root")
    args = parser.parse_args()
    specs = collector_specs(args.repo_root)
    if args.command == "list-collectors":
        print(json.dumps({"model_version": 1, "generated_at": now_iso(), "collectors": sorted(specs.keys())}, indent=2))
        return 0
    if args.command == "run":
        if args.collector not in specs:
            print(json.dumps({"error": f"unknown collector: {args.collector}", "available": sorted(specs.keys())}, indent=2))
            return 2
        print(json.dumps(build_capture(args.collector, args.repo_root, specs[args.collector]), indent=2))
        return 0
    captured_at = now_iso()
    scope = {"repo_root": args.repo_root}
    run_id = stable_run_id(scope, captured_at)
    print(json.dumps({"model_version": 1, "skill_key": "automation_engineering", "run_id": run_id, "captured_at": captured_at, "scope": scope, "collectors": [build_capture(name, args.repo_root, commands, run_id=run_id) for name, commands in specs.items()]}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
