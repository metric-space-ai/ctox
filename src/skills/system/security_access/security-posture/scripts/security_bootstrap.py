#!/usr/bin/env python3
import argparse
import json
import sqlite3
import sys
from pathlib import Path


import re


DATE_RE = re.compile(r"notAfter=(.+)")


def open_db(db_path: str) -> sqlite3.Connection:
    return sqlite3.connect(Path(db_path))


def load_captures(conn: sqlite3.Connection, run_id: str) -> list[dict]:
    rows = conn.execute(
        """
        SELECT capture_id, collector, tool, command_json, stdout_text, stderr_text, exit_code
        FROM discovery_capture
        WHERE run_id = ?
        ORDER BY collector, capture_id
        """,
        (run_id,),
    ).fetchall()
    return [
        {"capture_id": row[0], "collector": row[1], "tool": row[2], "argv": json.loads(row[3]), "stdout": row[4], "stderr": row[5], "exit_code": row[6]}
        for row in rows
    ]


def group_captures(captures: list[dict]) -> dict[tuple[str, str], list[dict]]:
    grouped: dict[tuple[str, str], list[dict]] = {}
    for item in captures:
        grouped.setdefault((item["collector"], item["tool"]), []).append(item)
    return grouped


def build_graph(conn: sqlite3.Connection, run_id: str) -> dict:
    captures = load_captures(conn, run_id)
    if not captures:
        raise SystemExit(f"no security_posture captures found for run_id={run_id}")
    grouped = group_captures(captures)
    snapshot = {"kind": "compliance_snapshot", "natural_key": f"compliance_snapshot:{run_id}", "title": "Security posture snapshot", "attrs": {"status": "partial"}}
    findings: list[dict] = []
    evidence: list[dict] = []
    remediation = {"kind": "remediation_plan", "natural_key": f"remediation_plan:{run_id}", "title": "Security remediation plan", "attrs": {"actions": []}}
    for item in grouped.get(("listeners", "ss"), []):
        for line in item["stdout"].splitlines():
            parts = line.split()
            bind_port = next((token for token in parts if ":" in token and not token.endswith(":*")), None)
            if not bind_port:
                continue
            bind, _, port = bind_port.rpartition(":")
            if not bind or not port.isdigit():
                continue
            if bind in ("0.0.0.0", "[::]"):
                natural_key = f"security_finding:listener:{bind}:{port}"
                findings.append({"kind": "security_finding", "natural_key": natural_key, "title": f"Public listener on {bind}:{port}", "attrs": {"category": "listener_exposure", "bind": bind, "port": int(port)}})
                evidence.append({"capture_id": item["capture_id"], "entity": {"kind": "security_finding", "natural_key": natural_key}, "note": f"socket bound to {bind}:{port}"})
    for item in grouped.get(("permissions", "find"), []):
        for line in item["stdout"].splitlines()[:5]:
            if line.strip():
                natural_key = f"security_finding:permissions:{abs(hash(line))}"
                findings.append({"kind": "security_finding", "natural_key": natural_key, "title": "Weak file permissions", "attrs": {"category": "file_permissions", "sample": line[:220]}})
                evidence.append({"capture_id": item["capture_id"], "entity": {"kind": "security_finding", "natural_key": natural_key}, "note": line[:220]})
    for item in grouped.get(("certificates", "openssl"), []):
        match = DATE_RE.search(item["stdout"])
        if match:
            natural_key = f"security_finding:certificate:{run_id}"
            findings.append({"kind": "security_finding", "natural_key": natural_key, "title": "Certificate observed", "attrs": {"category": "certificate", "not_after": match.group(1).strip()}})
            evidence.append({"capture_id": item["capture_id"], "entity": {"kind": "security_finding", "natural_key": natural_key}, "note": "certificate metadata captured"})
    if findings:
        remediation["attrs"]["actions"] = ["narrow public listeners", "tighten file permissions", "review certificate expiry"]
        snapshot["attrs"]["status"] = "findings_present"
    entities = [snapshot, remediation, *findings]
    relations = [{"from": {"kind": "remediation_plan", "natural_key": remediation["natural_key"]}, "relation": "derived_from", "to": {"kind": "compliance_snapshot", "natural_key": snapshot["natural_key"]}, "attrs": {}}]
    for finding in findings:
        relations.append({"from": {"kind": "security_finding", "natural_key": finding["natural_key"]}, "relation": "derived_from", "to": {"kind": "compliance_snapshot", "natural_key": snapshot["natural_key"]}, "attrs": {}})
    return {"run_id": run_id, "skill_key": "security_posture", "status": "normalized", "note": "security bootstrap graph", "entities": entities, "relations": relations, "evidence": evidence}


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a conservative security_posture graph from shared SQLite captures.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    conn = open_db(args.db)
    print(json.dumps(build_graph(conn, args.run_id), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
