#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
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


def collector_specs(
    unit: Optional[str],
    repo_root: Optional[str],
    config_path: Optional[str],
    url: Optional[str],
) -> Dict[str, List[Dict[str, Any]]]:
    specs: Dict[str, List[Dict[str, Any]]] = {
        "change_scope": [{"tool": "hostnamectl", "argv": ["hostnamectl"]}],
        "change_verify": [{"tool": "ss", "argv": ["ss", "-tulpnH"]}],
    }
    if unit:
        specs["change_scope"].extend(
            [
                {"tool": "systemctl", "argv": ["systemctl", "status", unit, "--no-pager"]},
                {"tool": "systemctl", "argv": ["systemctl", "cat", unit]},
            ]
        )
        specs["change_verify"].extend(
            [
                {"tool": "systemctl", "argv": ["systemctl", "status", unit, "--no-pager"]},
                {"tool": "journalctl", "argv": ["journalctl", "-u", unit, "-n", "120", "--no-pager"]},
            ]
        )
    if repo_root:
        specs["change_scope"].append({"tool": "git", "argv": ["git", "-C", repo_root, "status", "--short"]})
        specs["change_diff"] = [
            {"tool": "git", "argv": ["git", "-C", repo_root, "diff", "--stat"]},
            {"tool": "git", "argv": ["git", "-C", repo_root, "diff"]},
        ]
    else:
        specs["change_diff"] = []
    if config_path:
        specs["change_scope"].append({"tool": "stat", "argv": ["stat", config_path]})
        if os.path.exists(config_path):
            specs["change_diff"].append({"tool": "sh", "argv": ["sh", "-lc", f"sed -n '1,200p' {shell_quote(config_path)}"]})
    if url:
        specs["change_verify"].append({"tool": "curl", "argv": ["curl", "-sS", "-o", "/dev/null", "-w", "%{http_code} %{time_total}\n", "--max-time", "10", url]})
    if command_available("apt"):
        specs["change_diff"].append({"tool": "apt", "argv": ["apt", "list", "--upgradable"]})
    elif command_available("dnf"):
        specs["change_diff"].append({"tool": "dnf", "argv": ["dnf", "check-update"]})
    return specs


def shell_quote(value: str) -> str:
    return "'" + value.replace("'", "'\"'\"'") + "'"


def run_command(spec: Dict[str, Any]) -> Dict[str, Any]:
    argv = list(spec["argv"])
    started_at = now_iso()
    monotonic_start = time.monotonic()
    resolved = shutil.which(argv[0])
    if resolved is None:
        return {
            "tool": spec["tool"],
            "argv": argv,
            "started_at": started_at,
            "finished_at": now_iso(),
            "duration_ms": 0,
            "exit_code": None,
            "available": False,
            "stdout": "",
            "stderr": f"command not found: {argv[0]}",
        }
    completed = subprocess.run(argv, capture_output=True, text=True, check=False)
    return {
        "tool": spec["tool"],
        "argv": argv,
        "started_at": started_at,
        "finished_at": now_iso(),
        "duration_ms": int((time.monotonic() - monotonic_start) * 1000),
        "exit_code": completed.returncode,
        "available": True,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def build_capture(
    collector: str,
    unit: Optional[str],
    repo_root: Optional[str],
    config_path: Optional[str],
    url: Optional[str],
    commands: List[Dict[str, Any]],
    run_id: Optional[str] = None,
) -> Dict[str, Any]:
    captured_at = now_iso()
    scope = {"unit": unit, "repo_root": repo_root, "config_path": config_path, "url": url}
    return {
        "model_version": 1,
        "skill_key": "change_lifecycle",
        "run_id": run_id or stable_run_id(scope, captured_at),
        "captured_at": captured_at,
        "collector": collector,
        "scope": scope,
        "captures": [run_command(spec) for spec in commands],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run change-lifecycle collectors and return raw JSON captures.")
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name in ("list-collectors", "run", "run-all"):
        sub = subparsers.add_parser(name)
        if name == "run":
            sub.add_argument("--collector", required=True)
        sub.add_argument("--unit")
        sub.add_argument("--repo-root")
        sub.add_argument("--config-path")
        sub.add_argument("--url")
    args = parser.parse_args()
    specs = collector_specs(args.unit, args.repo_root, args.config_path, args.url)
    if args.command == "list-collectors":
        print(json.dumps({"model_version": 1, "generated_at": now_iso(), "collectors": sorted(specs.keys())}, indent=2))
        return 0
    if args.command == "run":
        if args.collector not in specs:
            print(json.dumps({"error": f"unknown collector: {args.collector}", "available": sorted(specs.keys())}, indent=2))
            return 2
        print(json.dumps(build_capture(args.collector, args.unit, args.repo_root, args.config_path, args.url, specs[args.collector]), indent=2))
        return 0
    captured_at = now_iso()
    scope = {"unit": args.unit, "repo_root": args.repo_root, "config_path": args.config_path, "url": args.url}
    run_id = stable_run_id(scope, captured_at)
    print(json.dumps({"model_version": 1, "skill_key": "change_lifecycle", "run_id": run_id, "captured_at": captured_at, "scope": scope, "collectors": [build_capture(name, args.unit, args.repo_root, args.config_path, args.url, commands, run_id=run_id) for name, commands in specs.items()]}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
