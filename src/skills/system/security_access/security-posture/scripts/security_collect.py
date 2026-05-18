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


def collector_specs(unit: Optional[str], cert_path: Optional[str], scan_path: Optional[str], host: Optional[str], port: Optional[str]) -> Dict[str, List[Dict[str, Any]]]:
    specs: Dict[str, List[Dict[str, Any]]] = {
        "accounts": [
            {"tool": "getent", "argv": ["getent", "passwd"]},
            {"tool": "getent", "argv": ["getent", "group"]},
        ],
        "listeners": [
            {"tool": "ss", "argv": ["ss", "-tulpnH"]},
            {"tool": "ip", "argv": ["ip", "-br", "addr"]},
            {"tool": "systemctl", "argv": ["systemctl", "list-units", "--type=socket", "--all", "--no-pager", "--no-legend", "--plain"]},
        ],
        "package_posture": [],
    }
    if command_available("ufw"):
        specs["firewall"] = [{"tool": "ufw", "argv": ["ufw", "status", "verbose"]}]
    elif command_available("nft"):
        specs["firewall"] = [{"tool": "nft", "argv": ["nft", "list", "ruleset"]}]
    elif command_available("iptables"):
        specs["firewall"] = [{"tool": "iptables", "argv": ["iptables", "-S"]}]
    else:
        specs["firewall"] = []
    if unit:
        specs["service_hardening"] = [
            {"tool": "systemctl", "argv": ["systemctl", "cat", unit]},
            {"tool": "systemd-analyze", "argv": ["systemd-analyze", "security", unit]},
        ]
    if cert_path:
        specs["certificates"] = [{"tool": "openssl", "argv": ["openssl", "x509", "-in", cert_path, "-noout", "-subject", "-issuer", "-dates"]}]
    elif host and port:
        specs["certificates"] = [{"tool": "openssl", "argv": ["sh", "-lc", f"openssl s_client -connect {host}:{port} -servername {host} </dev/null"]}]
    if scan_path:
        specs["permissions"] = [
            {"tool": "find", "argv": ["find", scan_path, "-xdev", "-type", "f", "(", "-name", "*.env", "-o", "-name", "*.pem", "-o", "-name", "*.key", ")", "-ls"]},
            {"tool": "find", "argv": ["find", scan_path, "-xdev", "-type", "f", "-perm", "-0002"]},
        ]
    else:
        specs["permissions"] = [{"tool": "find", "argv": ["find", "/etc", "-xdev", "-type", "f", "-perm", "-0002"]}]
    if command_available("apt"):
        specs["package_posture"].append({"tool": "apt", "argv": ["apt", "list", "--installed"]})
    elif command_available("dnf"):
        specs["package_posture"].append({"tool": "dnf", "argv": ["dnf", "list", "installed"]})
    elif command_available("rpm"):
        specs["package_posture"].append({"tool": "rpm", "argv": ["rpm", "-qa"]})
    elif command_available("dpkg"):
        specs["package_posture"].append({"tool": "dpkg", "argv": ["dpkg", "-l"]})
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


def build_capture(collector: str, unit: Optional[str], cert_path: Optional[str], scan_path: Optional[str], host: Optional[str], port: Optional[str], commands: List[Dict[str, Any]], run_id: Optional[str] = None) -> Dict[str, Any]:
    captured_at = now_iso()
    scope = {"unit": unit, "cert_path": cert_path, "scan_path": scan_path, "host": host, "port": port}
    return {"model_version": 1, "skill_key": "security_posture", "run_id": run_id or stable_run_id(scope, captured_at), "captured_at": captured_at, "collector": collector, "scope": scope, "captures": [run_command(spec) for spec in commands]}


def main() -> int:
    parser = argparse.ArgumentParser(description="Run security-posture collectors and return raw JSON captures.")
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name in ("list-collectors", "run", "run-all"):
        sub = subparsers.add_parser(name)
        if name == "run":
            sub.add_argument("--collector", required=True)
        sub.add_argument("--unit")
        sub.add_argument("--cert-path")
        sub.add_argument("--scan-path")
        sub.add_argument("--host")
        sub.add_argument("--port")
    args = parser.parse_args()
    specs = collector_specs(args.unit, args.cert_path, args.scan_path, args.host, args.port)
    if args.command == "list-collectors":
        print(json.dumps({"model_version": 1, "generated_at": now_iso(), "collectors": sorted(specs.keys())}, indent=2))
        return 0
    if args.command == "run":
        if args.collector not in specs:
            print(json.dumps({"error": f"unknown collector: {args.collector}", "available": sorted(specs.keys())}, indent=2))
            return 2
        print(json.dumps(build_capture(args.collector, args.unit, args.cert_path, args.scan_path, args.host, args.port, specs[args.collector]), indent=2))
        return 0
    captured_at = now_iso()
    scope = {"unit": args.unit, "cert_path": args.cert_path, "scan_path": args.scan_path, "host": args.host, "port": args.port}
    run_id = stable_run_id(scope, captured_at)
    print(json.dumps({"model_version": 1, "skill_key": "security_posture", "run_id": run_id, "captured_at": captured_at, "scope": scope, "collectors": [build_capture(name, args.unit, args.cert_path, args.scan_path, args.host, args.port, commands, run_id=run_id) for name, commands in specs.items()]}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
