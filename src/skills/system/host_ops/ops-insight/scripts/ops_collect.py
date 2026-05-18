#!/usr/bin/env python3
import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def stable_run_id(scope: dict[str, Any], captured_at: str) -> str:
    payload = json.dumps({"scope": scope, "captured_at": captured_at}, sort_keys=True)
    digest = hashlib.sha256(payload.encode("utf-8")).hexdigest()[:16]
    return f"run-{digest}"


def command_available(name: str) -> bool:
    return shutil.which(name) is not None


def collector_specs(db_path: Optional[str]) -> Dict[str, List[Dict[str, Any]]]:
    specs: Dict[str, List[Dict[str, Any]]] = {
        "ctox_state": [],
        "host_brief": [
            {"tool": "uptime", "argv": ["uptime"]},
            {"tool": "free", "argv": ["free", "-h"]},
            {"tool": "df", "argv": ["df", "-h"]},
            {"tool": "ss", "argv": ["ss", "-s"]},
            {"tool": "journalctl", "argv": ["journalctl", "-p", "err", "-n", "100", "--no-pager"]},
        ],
        "kernel_summary": [],
    }
    if command_available("ctox"):
        specs["ctox_state"].extend(
            [
                {"tool": "ctox", "argv": ["ctox", "queue", "list", "--limit", "50"]},
                {"tool": "ctox", "argv": ["ctox", "plan", "list"]},
                {"tool": "ctox", "argv": ["ctox", "schedule", "list"]},
                {"tool": "ctox", "argv": ["ctox", "status"]},
            ]
        )
    if db_path:
        skill_root = Path(__file__).resolve().parents[2]
        discovery_query = skill_root / "discovery-graph" / "scripts" / "discovery_query.py"
        reliability_query = skill_root / "reliability-ops" / "scripts" / "reliability_query.py"
        specs["kernel_summary"].append({"tool": "python3", "argv": [sys.executable or "python3", str(discovery_query), "summary", "--db", db_path]})
        specs["kernel_summary"].append({"tool": "python3", "argv": [sys.executable or "python3", str(reliability_query), "summary", "--db", db_path]})
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


def build_capture(collector: str, db_path: Optional[str], commands: List[Dict[str, Any]], run_id: Optional[str] = None) -> Dict[str, Any]:
    captured_at = now_iso()
    scope = {"db_path": db_path}
    return {"model_version": 1, "skill_key": "ops_insight", "run_id": run_id or stable_run_id(scope, captured_at), "captured_at": captured_at, "collector": collector, "scope": scope, "captures": [run_command(spec) for spec in commands]}


def main() -> int:
    parser = argparse.ArgumentParser(description="Run ops-insight collectors and return raw JSON captures.")
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name in ("list-collectors", "run", "run-all"):
        sub = subparsers.add_parser(name)
        if name == "run":
            sub.add_argument("--collector", required=True)
        sub.add_argument("--db-path")
    args = parser.parse_args()
    specs = collector_specs(args.db_path)
    if args.command == "list-collectors":
        print(json.dumps({"model_version": 1, "generated_at": now_iso(), "collectors": sorted(specs.keys())}, indent=2))
        return 0
    if args.command == "run":
        if args.collector not in specs:
            print(json.dumps({"error": f"unknown collector: {args.collector}", "available": sorted(specs.keys())}, indent=2))
            return 2
        print(json.dumps(build_capture(args.collector, args.db_path, specs[args.collector]), indent=2))
        return 0
    captured_at = now_iso()
    scope = {"db_path": args.db_path}
    run_id = stable_run_id(scope, captured_at)
    print(json.dumps({"model_version": 1, "skill_key": "ops_insight", "run_id": run_id, "captured_at": captured_at, "scope": scope, "collectors": [build_capture(name, args.db_path, commands, run_id=run_id) for name, commands in specs.items()]}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
