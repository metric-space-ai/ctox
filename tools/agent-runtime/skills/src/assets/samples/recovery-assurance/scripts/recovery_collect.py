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


def collector_specs(backup_unit: Optional[str], backup_path: Optional[str], repo: Optional[str], dump_path: Optional[str]) -> Dict[str, List[Dict[str, Any]]]:
    specs: Dict[str, List[Dict[str, Any]]] = {
        "scheduler": [
            {"tool": "systemctl", "argv": ["systemctl", "list-timers", "--all", "--no-pager", "--no-legend", "--plain"]},
            {"tool": "sh", "argv": ["sh", "-lc", "crontab -l 2>/dev/null || true"]},
        ],
        "filesystem_backup": [],
        "snapshot_repo": [],
        "database_backup": [],
    }
    if backup_unit:
        specs["scheduler"].extend(
            [
                {"tool": "systemctl", "argv": ["systemctl", "status", backup_unit, "--no-pager"]},
                {"tool": "journalctl", "argv": ["journalctl", "-u", backup_unit, "-n", "200", "--no-pager"]},
            ]
        )
    if backup_path:
        specs["filesystem_backup"].append({"tool": "ls", "argv": ["ls", "-lh", backup_path]})
        if backup_path.endswith(".tar") or backup_path.endswith(".tar.gz") or backup_path.endswith(".tgz"):
            specs["filesystem_backup"].append({"tool": "tar", "argv": ["tar", "-tf", backup_path]})
        specs["filesystem_backup"].append({"tool": "sha256sum", "argv": ["sha256sum", backup_path]})
    if repo:
        if command_available("restic"):
            specs["snapshot_repo"].extend([{"tool": "restic", "argv": ["restic", "snapshots"]}, {"tool": "restic", "argv": ["restic", "check"]}])
        if command_available("borg"):
            specs["snapshot_repo"].extend([{"tool": "borg", "argv": ["borg", "list", repo]}, {"tool": "borg", "argv": ["borg", "check", repo]}])
        if command_available("rclone"):
            specs["snapshot_repo"].append({"tool": "rclone", "argv": ["rclone", "ls", repo]})
    if dump_path:
        if command_available("pg_restore"):
            specs["database_backup"].append({"tool": "pg_restore", "argv": ["pg_restore", "--list", dump_path]})
        if command_available("pg_dump"):
            specs["database_backup"].append({"tool": "pg_dump", "argv": ["pg_dump", "--version"]})
        if command_available("mysqldump"):
            specs["database_backup"].append({"tool": "mysqldump", "argv": ["mysqldump", "--version"]})
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


def build_capture(collector: str, backup_unit: Optional[str], backup_path: Optional[str], repo: Optional[str], dump_path: Optional[str], commands: List[Dict[str, Any]], run_id: Optional[str] = None) -> Dict[str, Any]:
    captured_at = now_iso()
    scope = {"backup_unit": backup_unit, "backup_path": backup_path, "repo": repo, "dump_path": dump_path}
    return {"model_version": 1, "skill_key": "recovery_assurance", "run_id": run_id or stable_run_id(scope, captured_at), "captured_at": captured_at, "collector": collector, "scope": scope, "captures": [run_command(spec) for spec in commands]}


def main() -> int:
    parser = argparse.ArgumentParser(description="Run recovery-assurance collectors and return raw JSON captures.")
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name in ("list-collectors", "run", "run-all"):
        sub = subparsers.add_parser(name)
        if name == "run":
            sub.add_argument("--collector", required=True)
        sub.add_argument("--backup-unit")
        sub.add_argument("--backup-path")
        sub.add_argument("--repo")
        sub.add_argument("--dump-path")
    args = parser.parse_args()
    specs = collector_specs(args.backup_unit, args.backup_path, args.repo, args.dump_path)
    if args.command == "list-collectors":
        print(json.dumps({"model_version": 1, "generated_at": now_iso(), "collectors": sorted(specs.keys())}, indent=2))
        return 0
    if args.command == "run":
        if args.collector not in specs:
            print(json.dumps({"error": f"unknown collector: {args.collector}", "available": sorted(specs.keys())}, indent=2))
            return 2
        print(json.dumps(build_capture(args.collector, args.backup_unit, args.backup_path, args.repo, args.dump_path, specs[args.collector]), indent=2))
        return 0
    captured_at = now_iso()
    scope = {"backup_unit": args.backup_unit, "backup_path": args.backup_path, "repo": args.repo, "dump_path": args.dump_path}
    run_id = stable_run_id(scope, captured_at)
    print(json.dumps({"model_version": 1, "skill_key": "recovery_assurance", "run_id": run_id, "captured_at": captured_at, "scope": scope, "collectors": [build_capture(name, args.backup_unit, args.backup_path, args.repo, args.dump_path, commands, run_id=run_id) for name, commands in specs.items()]}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
