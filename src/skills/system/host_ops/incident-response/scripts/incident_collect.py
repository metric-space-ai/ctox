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


def collector_specs(
    unit: Optional[str],
    url: Optional[str],
    host: Optional[str],
    repo_root: Optional[str],
) -> Dict[str, List[Dict[str, Any]]]:
    specs: Dict[str, List[Dict[str, Any]]] = {
        "incident_overview": [
            {"tool": "hostnamectl", "argv": ["hostnamectl"]},
            {"tool": "uptime", "argv": ["uptime"]},
            {"tool": "free", "argv": ["free", "-m"]},
            {"tool": "df", "argv": ["df", "-P", "-h"]},
            {"tool": "ss", "argv": ["ss", "-s"]},
        ],
        "incident_logs": [
            {"tool": "journalctl", "argv": ["journalctl", "-p", "warning", "-n", "160", "--no-pager"]},
        ],
        "recent_change": [
            {
                "tool": "systemctl",
                "argv": ["systemctl", "list-timers", "--all", "--no-pager", "--no-legend", "--plain"],
            }
        ],
    }
    if unit:
        specs["incident_service"] = [
            {"tool": "systemctl", "argv": ["systemctl", "status", unit, "--no-pager"]},
            {"tool": "systemctl", "argv": ["systemctl", "--user", "status", unit, "--no-pager"]},
        ]
        specs["incident_logs"].append(
            {
                "tool": "journalctl",
                "argv": ["journalctl", "-u", unit, "--since", "30 min ago", "--no-pager"],
            }
        )
    if url:
        specs["dependency_probe"] = [
            {"tool": "curl", "argv": ["curl", "-I", "-sS", "--max-time", "10", url]},
        ]
    if host:
        specs.setdefault("dependency_probe", []).extend(
            [
                {"tool": "ping", "argv": ["ping", "-c", "3", host]},
                {"tool": "dig", "argv": ["dig", host]},
            ]
        )
    if repo_root:
        specs["recent_change"].extend(
            [
                {"tool": "git", "argv": ["git", "-C", repo_root, "log", "--oneline", "-n", "10"]},
                {
                    "tool": "sh",
                    "argv": ["sh", "-lc", f"ls -lt {shlex_quote(repo_root)} | head -n 20"],
                },
            ]
        )
    return specs


def shlex_quote(value: str) -> str:
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
    url: Optional[str],
    host: Optional[str],
    repo_root: Optional[str],
    commands: List[Dict[str, Any]],
    run_id: Optional[str] = None,
) -> Dict[str, Any]:
    captured_at = now_iso()
    scope = {"unit": unit, "url": url, "host": host, "repo_root": repo_root}
    return {
        "model_version": 1,
        "skill_key": "incident_response",
        "run_id": run_id or stable_run_id(scope, captured_at),
        "captured_at": captured_at,
        "collector": collector,
        "scope": scope,
        "captures": [run_command(spec) for spec in commands],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run incident-response collectors and return raw JSON captures.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    for name in ("list-collectors", "run", "run-all"):
        sub = subparsers.add_parser(name)
        if name == "run":
            sub.add_argument("--collector", required=True)
        sub.add_argument("--unit")
        sub.add_argument("--url")
        sub.add_argument("--host")
        sub.add_argument("--repo-root")

    args = parser.parse_args()
    specs = collector_specs(args.unit, args.url, args.host, args.repo_root)

    if args.command == "list-collectors":
        print(json.dumps({"model_version": 1, "generated_at": now_iso(), "collectors": sorted(specs.keys())}, indent=2))
        return 0

    if args.command == "run":
        if args.collector not in specs:
            print(json.dumps({"error": f"unknown collector: {args.collector}", "available": sorted(specs.keys())}, indent=2))
            return 2
        print(json.dumps(build_capture(args.collector, args.unit, args.url, args.host, args.repo_root, specs[args.collector]), indent=2))
        return 0

    captured_at = now_iso()
    scope = {"unit": args.unit, "url": args.url, "host": args.host, "repo_root": args.repo_root}
    run_id = stable_run_id(scope, captured_at)
    combined = {
        "model_version": 1,
        "skill_key": "incident_response",
        "run_id": run_id,
        "captured_at": captured_at,
        "scope": scope,
        "collectors": [
            build_capture(name, args.unit, args.url, args.host, args.repo_root, commands, run_id=run_id)
            for name, commands in specs.items()
        ],
    }
    print(json.dumps(combined, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
