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


def stable_run_id(repo_root: Optional[str], captured_at: str) -> str:
    payload = json.dumps(
        {
            "repo_root": str(Path(repo_root).resolve()) if repo_root else None,
            "captured_at": captured_at,
        },
        sort_keys=True,
    )
    digest = hashlib.sha256(payload.encode("utf-8")).hexdigest()[:16]
    return f"run-{digest}"


def collector_specs(repo_root: Optional[str]) -> Dict[str, List[Dict[str, Any]]]:
    repo_path = str(Path(repo_root).resolve()) if repo_root else None
    specs: Dict[str, List[Dict[str, Any]]] = {
        "host_identity": [
            {"tool": "hostnamectl", "argv": ["hostnamectl"]},
            {"tool": "uname", "argv": ["uname", "-a"]},
            {"tool": "uptime", "argv": ["uptime"]},
        ],
        "network": [
            {"tool": "ip", "argv": ["ip", "-json", "address", "show"]},
            {"tool": "ip", "argv": ["ip", "route", "show"]},
            {"tool": "ss", "argv": ["ss", "-s"]},
        ],
        "listeners": [
            {"tool": "ss", "argv": ["ss", "-tulpnH"]},
        ],
        "services": [
            {
                "tool": "systemctl",
                "argv": [
                    "systemctl",
                    "list-units",
                    "--type=service",
                    "--all",
                    "--no-pager",
                    "--no-legend",
                    "--plain",
                ],
            },
            {
                "tool": "systemctl",
                "argv": [
                    "systemctl",
                    "show",
                    "--type=service",
                    "--all",
                    "--property",
                    "Id,Names,LoadState,ActiveState,SubState,MainPID,FragmentPath,Description",
                ],
            },
            {
                "tool": "systemctl",
                "argv": [
                    "systemctl",
                    "--user",
                    "list-units",
                    "--type=service",
                    "--all",
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
                    "show",
                    "--type=service",
                    "--all",
                    "--property",
                    "Id,Names,LoadState,ActiveState,SubState,MainPID,FragmentPath,Description",
                ],
            },
            {
                "tool": "systemctl",
                "argv": [
                    "systemctl",
                    "list-timers",
                    "--all",
                    "--no-pager",
                    "--no-legend",
                    "--plain",
                ],
            },
            {
                "tool": "systemctl",
                "argv": [
                    "systemctl",
                    "show",
                    "--type=timer",
                    "--all",
                    "--property",
                    "Id,Names,Unit,NextElapseUSecRealtime,LastTriggerUSec,FragmentPath,Description",
                ],
            },
            {
                "tool": "systemctl",
                "argv": [
                    "systemctl",
                    "--user",
                    "list-timers",
                    "--all",
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
                    "show",
                    "--type=timer",
                    "--all",
                    "--property",
                    "Id,Names,Unit,NextElapseUSecRealtime,LastTriggerUSec,FragmentPath,Description",
                ],
            },
        ],
        "journals": [
            {
                "tool": "systemctl",
                "argv": [
                    "systemctl",
                    "--failed",
                    "--no-pager",
                    "--no-legend",
                    "--plain",
                ],
            },
            {
                "tool": "journalctl",
                "argv": ["journalctl", "-p", "warning", "-n", "200", "--no-pager"],
            },
        ],
        "processes": [
            {
                "tool": "ps",
                "argv": [
                    "ps",
                    "-eo",
                    "pid=,ppid=,user=,stat=,pcpu=,pmem=,comm=,args=",
                ],
            },
            {
                "tool": "bash",
                "argv": [
                    "bash",
                    "-lc",
                    "for pid in $(ps -eo pid=); do "
                    "file=/proc/$pid/cgroup; "
                    "[ -r \"$file\" ] || continue; "
                    "printf 'PID=%s\\n' \"$pid\"; "
                    "cat \"$file\"; "
                    "printf '\\n'; "
                    "done",
                ],
            },
        ],
        "storage": [
            {"tool": "findmnt", "argv": ["findmnt", "--json"]},
            {"tool": "lsblk", "argv": ["lsblk", "--json", "-f"]},
            {"tool": "df", "argv": ["df", "-h"]},
        ],
        "containers": [
            {
                "tool": "docker",
                "argv": [
                    "docker",
                    "ps",
                    "--format",
                    "json",
                ],
            },
            {
                "tool": "podman",
                "argv": [
                    "podman",
                    "ps",
                    "--format",
                    "json",
                ],
            },
        ],
        "kubernetes": [
            {"tool": "kubectl", "argv": ["kubectl", "get", "nodes", "-o", "json"]},
            {"tool": "kubectl", "argv": ["kubectl", "get", "pods", "-A", "-o", "json"]},
            {"tool": "kubectl", "argv": ["kubectl", "get", "svc", "-A", "-o", "json"]},
        ],
    }
    if repo_path:
        specs["repo_inventory"] = [
            {"tool": "rg", "argv": ["rg", "--files"], "cwd": repo_path},
            {
                "tool": "rg",
                "argv": [
                    "rg",
                    "-n",
                    "docker|compose|systemd|service|timer|socket|listen|port|health|postgres|mysql|redis|kubernetes|helm",
                ],
                "cwd": repo_path,
            },
        ]
    return specs


def list_collectors(repo_root: Optional[str]) -> Dict[str, Any]:
    specs = collector_specs(repo_root)
    return {
        "model_version": 1,
        "generated_at": now_iso(),
        "collectors": sorted(specs.keys()),
    }


def run_command(spec: Dict[str, Any]) -> Dict[str, Any]:
    argv = list(spec["argv"])
    tool = str(spec["tool"])
    cwd = spec.get("cwd")
    started_at = now_iso()
    monotonic_start = time.monotonic()
    resolved = shutil.which(argv[0])
    if resolved is None:
        return {
            "tool": tool,
            "argv": argv,
            "cwd": cwd,
            "started_at": started_at,
            "finished_at": now_iso(),
            "duration_ms": 0,
            "exit_code": None,
            "available": False,
            "stdout": "",
            "stderr": f"command not found: {argv[0]}",
        }
    completed = subprocess.run(
        argv,
        cwd=cwd,
        capture_output=True,
        text=True,
        check=False,
    )
    finished_at = now_iso()
    duration_ms = int((time.monotonic() - monotonic_start) * 1000)
    return {
        "tool": tool,
        "argv": argv,
        "cwd": cwd,
        "started_at": started_at,
        "finished_at": finished_at,
        "duration_ms": duration_ms,
        "exit_code": completed.returncode,
        "available": True,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }


def extract_service_units(text: str) -> List[str]:
    units: List[str] = []
    for line in text.splitlines():
        parts = line.strip().split()
        if not parts:
            continue
        unit_id = parts[0]
        if unit_id.endswith(".service") and unit_id not in units:
            units.append(unit_id)
    return units


def extract_timer_units(text: str) -> List[str]:
    units: List[str] = []
    for line in text.splitlines():
        parts = line.strip().split()
        if len(parts) < 2:
            continue
        timer_id = parts[-2]
        if timer_id.endswith(".timer") and timer_id not in units:
            units.append(timer_id)
    return units


def build_capture(
    collector: str,
    repo_root: Optional[str],
    commands: List[Dict[str, Any]],
    run_id: Optional[str] = None,
) -> Dict[str, Any]:
    captured_at = now_iso()
    captures = [run_command(spec) for spec in commands]
    if collector == "services":
        seen = {tuple(capture.get("argv", [])) for capture in captures}
        service_units: List[tuple[List[str], str]] = []
        timer_units: List[tuple[List[str], str]] = []
        for capture in captures:
            argv = capture.get("argv", [])
            prefix = ["systemctl"]
            if argv[:2] == ["systemctl", "--user"]:
                prefix = ["systemctl", "--user"]
            if argv[: len(prefix) + 1] == prefix + ["list-units"]:
                service_units.extend((prefix, unit_id) for unit_id in extract_service_units(capture.get("stdout", "")))
            elif argv[: len(prefix) + 1] == prefix + ["list-timers"]:
                timer_units.extend((prefix, timer_id) for timer_id in extract_timer_units(capture.get("stdout", "")))
        for prefix, unit_id in service_units:
            spec = {
                "tool": "systemctl",
                "argv": prefix
                + [
                    "show",
                    unit_id,
                    "--property",
                    "Id,Names,LoadState,ActiveState,SubState,MainPID,FragmentPath,Description",
                ],
            }
            key = tuple(spec["argv"])
            if key not in seen:
                captures.append(run_command(spec))
                seen.add(key)
        for prefix, timer_id in timer_units:
            spec = {
                "tool": "systemctl",
                "argv": prefix
                + [
                    "show",
                    timer_id,
                    "--property",
                    "Id,Names,Unit,NextElapseUSecRealtime,LastTriggerUSec,FragmentPath,Description",
                ],
            }
            key = tuple(spec["argv"])
            if key not in seen:
                captures.append(run_command(spec))
                seen.add(key)
    return {
        "model_version": 1,
        "run_id": run_id or stable_run_id(repo_root, captured_at),
        "captured_at": captured_at,
        "collector": collector,
        "repo_root": str(Path(repo_root).resolve()) if repo_root else None,
        "captures": captures,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run discovery collectors and return raw JSON captures.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    list_parser = subparsers.add_parser("list-collectors")
    list_parser.add_argument("--repo-root")

    run_parser = subparsers.add_parser("run")
    run_parser.add_argument("--collector", required=True)
    run_parser.add_argument("--repo-root")

    run_all_parser = subparsers.add_parser("run-all")
    run_all_parser.add_argument("--repo-root")

    args = parser.parse_args()

    if args.command == "list-collectors":
        print(json.dumps(list_collectors(args.repo_root), indent=2))
        return 0

    specs = collector_specs(getattr(args, "repo_root", None))
    if args.command == "run":
        collector = args.collector
        if collector not in specs:
            print(json.dumps({"error": f"unknown collector: {collector}", "available": sorted(specs.keys())}, indent=2))
            return 2
        print(json.dumps(build_capture(collector, args.repo_root, specs[collector]), indent=2))
        return 0

    captured_at = now_iso()
    run_id = stable_run_id(args.repo_root, captured_at)
    combined = {
        "model_version": 1,
        "run_id": run_id,
        "captured_at": captured_at,
        "repo_root": str(Path(args.repo_root).resolve()) if args.repo_root else None,
        "collectors": [
            build_capture(name, args.repo_root, commands, run_id=run_id)
            for name, commands in specs.items()
        ],
    }
    print(json.dumps(combined, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
