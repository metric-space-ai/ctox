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


def collector_specs(unit: Optional[str], url: Optional[str]) -> Dict[str, List[Dict[str, Any]]]:
    specs: Dict[str, List[Dict[str, Any]]] = {
        "cpu_memory": [
            {"tool": "hostnamectl", "argv": ["hostnamectl"]},
            {"tool": "uptime", "argv": ["uptime"]},
            {"tool": "free", "argv": ["free", "-m"]},
            {"tool": "vmstat", "argv": ["vmstat", "1", "3"]},
            {"tool": "top", "argv": ["top", "-b", "-n", "1"]},
        ],
        "disk_io": [
            {"tool": "df", "argv": ["df", "-P", "-h"]},
            {"tool": "findmnt", "argv": ["findmnt"]},
            {"tool": "iostat", "argv": ["iostat", "-xz", "1", "2"]},
        ],
        "network_pressure": [
            {"tool": "ss", "argv": ["ss", "-s"]},
            {"tool": "ss", "argv": ["ss", "-tulpnH"]},
            {"tool": "ip", "argv": ["ip", "-json", "address", "show"]},
        ],
        "service_status": [
            {
                "tool": "systemctl",
                "argv": ["systemctl", "--failed", "--no-pager", "--no-legend", "--plain"],
            },
            {
                "tool": "systemctl",
                "argv": [
                    "systemctl",
                    "list-units",
                    "--type=service",
                    "--state=running",
                    "--no-pager",
                    "--no-legend",
                    "--plain",
                ],
            },
            {
                "tool": "systemctl",
                "argv": [
                    "systemctl",
                    "--user",
                    "list-units",
                    "--type=service",
                    "--state=running",
                    "--no-pager",
                    "--no-legend",
                    "--plain",
                ],
            },
        ],
        "service_logs": [
            {"tool": "journalctl", "argv": ["journalctl", "-p", "warning", "-n", "160", "--no-pager"]},
        ],
        "gpu_status": [
            {
                "tool": "nvidia-smi",
                "argv": [
                    "nvidia-smi",
                    "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu",
                    "--format=csv,noheader,nounits",
                ],
            }
        ],
    }
    if unit:
        specs["service_status"].append(
            {"tool": "systemctl", "argv": ["systemctl", "status", unit, "--no-pager"]}
        )
        specs["service_status"].append(
            {"tool": "systemctl", "argv": ["systemctl", "--user", "status", unit, "--no-pager"]}
        )
        specs["service_logs"].append(
            {"tool": "journalctl", "argv": ["journalctl", "-u", unit, "-n", "120", "--no-pager"]}
        )
    if url:
        specs["endpoint_probe"] = [
            {
                "tool": "curl",
                "argv": [
                    "curl",
                    "-sS",
                    "-o",
                    "/dev/null",
                    "-w",
                    "%{http_code} %{time_total}\n",
                    "--max-time",
                    "10",
                    url,
                ],
            }
        ]
    return specs


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
    commands: List[Dict[str, Any]],
    run_id: Optional[str] = None,
) -> Dict[str, Any]:
    captured_at = now_iso()
    scope = {"unit": unit, "url": url}
    return {
        "model_version": 1,
        "skill_key": "reliability_ops",
        "run_id": run_id or stable_run_id(scope, captured_at),
        "captured_at": captured_at,
        "collector": collector,
        "scope": scope,
        "captures": [run_command(spec) for spec in commands],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run reliability collectors and return raw JSON captures.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    for name in ("list-collectors", "run", "run-all"):
        sub = subparsers.add_parser(name)
        if name == "run":
            sub.add_argument("--collector", required=True)
        sub.add_argument("--unit")
        sub.add_argument("--url")

    args = parser.parse_args()
    specs = collector_specs(args.unit, args.url)

    if args.command == "list-collectors":
        print(json.dumps({"model_version": 1, "generated_at": now_iso(), "collectors": sorted(specs.keys())}, indent=2))
        return 0

    if args.command == "run":
        if args.collector not in specs:
            print(json.dumps({"error": f"unknown collector: {args.collector}", "available": sorted(specs.keys())}, indent=2))
            return 2
        print(json.dumps(build_capture(args.collector, args.unit, args.url, specs[args.collector]), indent=2))
        return 0

    captured_at = now_iso()
    scope = {"unit": args.unit, "url": args.url}
    run_id = stable_run_id(scope, captured_at)
    combined = {
        "model_version": 1,
        "skill_key": "reliability_ops",
        "run_id": run_id,
        "captured_at": captured_at,
        "scope": scope,
        "collectors": [
            build_capture(name, args.unit, args.url, commands, run_id=run_id)
            for name, commands in specs.items()
        ],
    }
    print(json.dumps(combined, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
