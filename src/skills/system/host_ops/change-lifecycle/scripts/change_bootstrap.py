#!/usr/bin/env python3
import argparse
import json
import re
import sqlite3
import sys
from pathlib import Path


HOSTNAME_RE = re.compile(r"Static hostname:\s*(\S+)")


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


def parse_hostname(grouped: dict[tuple[str, str], list[dict]]) -> str:
    for item in grouped.get(("change_scope", "hostnamectl"), []):
        match = HOSTNAME_RE.search(item["stdout"])
        if match:
            return match.group(1)
    return "unknown-host"


def first_unit(captures: list[dict]) -> str | None:
    for item in captures:
        for token in item["argv"]:
            if token.endswith(".service"):
                return token
    return None


def build_graph(conn: sqlite3.Connection, run_id: str) -> dict:
    captures = load_captures(conn, run_id)
    if not captures:
        raise SystemExit(f"no change_lifecycle captures found for run_id={run_id}")
    grouped = group_captures(captures)
    host_name = parse_hostname(grouped)
    unit = first_unit(captures)
    change_request = {"kind": "change_request", "natural_key": f"change_request:{run_id}", "title": f"Change request on {host_name}", "attrs": {"mode": "dry_run", "unit": unit}}
    config_snapshot = {"kind": "config_snapshot", "natural_key": f"config_snapshot:{run_id}", "title": "Pre-change snapshot", "attrs": {"captured": True}}
    change_plan = {"kind": "change_plan", "natural_key": f"change_plan:{run_id}", "title": "Dry-run change plan", "attrs": {"steps": ["capture current state", "diff current/target state", "prepare rollback", "verify after change"], "unit": unit}}
    rollback_bundle = {"kind": "rollback_bundle", "natural_key": f"rollback_bundle:{run_id}", "title": "Rollback bundle", "attrs": {"required": ["previous config or package state", "verification command"], "ready": unit is not None}}
    change_result = {"kind": "change_result", "natural_key": f"change_result:{run_id}", "title": "Change dry-run result", "attrs": {"status": "planned_only", "executed": False}}
    entities = [change_request, config_snapshot, change_plan, rollback_bundle, change_result]
    relations = [
        {"from": {"kind": "config_snapshot", "natural_key": config_snapshot["natural_key"]}, "relation": "derived_from", "to": {"kind": "change_request", "natural_key": change_request["natural_key"]}, "attrs": {}},
        {"from": {"kind": "change_plan", "natural_key": change_plan["natural_key"]}, "relation": "derived_from", "to": {"kind": "change_request", "natural_key": change_request["natural_key"]}, "attrs": {}},
        {"from": {"kind": "rollback_bundle", "natural_key": rollback_bundle["natural_key"]}, "relation": "derived_from", "to": {"kind": "change_plan", "natural_key": change_plan["natural_key"]}, "attrs": {}},
        {"from": {"kind": "change_result", "natural_key": change_result["natural_key"]}, "relation": "derived_from", "to": {"kind": "change_plan", "natural_key": change_plan["natural_key"]}, "attrs": {}},
    ]
    evidence = []
    for item in captures[:4]:
        evidence.append({"capture_id": item["capture_id"], "entity": {"kind": "change_plan", "natural_key": change_plan["natural_key"]}, "note": "pre-change evidence captured"})
    return {"run_id": run_id, "skill_key": "change_lifecycle", "status": "normalized", "note": "change bootstrap graph", "entities": entities, "relations": relations, "evidence": evidence}


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a conservative change_lifecycle graph from shared SQLite captures.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    conn = open_db(args.db)
    print(json.dumps(build_graph(conn, args.run_id), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
