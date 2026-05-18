#!/usr/bin/env python3
import argparse
import json
import sqlite3
import sys
from pathlib import Path


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


def build_graph(conn: sqlite3.Connection, run_id: str) -> dict:
    captures = load_captures(conn, run_id)
    if not captures:
        raise SystemExit(f"no ops_insight captures found for run_id={run_id}")
    scorecard = {"kind": "scorecard", "natural_key": f"scorecard:{run_id}", "title": "Ops scorecard", "attrs": {"collector_count": len(captures)}}
    brief = {"kind": "decision_brief", "natural_key": f"decision_brief:{run_id}", "title": "Decision brief", "attrs": {"summary": "Use discovery and reliability state to rank next actions."}}
    backlog = {"kind": "priority_backlog", "natural_key": f"priority_backlog:{run_id}", "title": "Priority backlog", "attrs": {"top_actions": ["stabilize degraded services", "close open recovery gaps", "promote repeated work into automation"]}}
    dashboard = {"kind": "dashboard_view", "natural_key": f"dashboard_view:{run_id}", "title": "Dashboard view", "attrs": {"format": "compact"}}
    entities = [scorecard, brief, backlog, dashboard]
    relations = [
        {"from": {"kind": "decision_brief", "natural_key": brief["natural_key"]}, "relation": "derived_from", "to": {"kind": "scorecard", "natural_key": scorecard["natural_key"]}, "attrs": {}},
        {"from": {"kind": "priority_backlog", "natural_key": backlog["natural_key"]}, "relation": "derived_from", "to": {"kind": "decision_brief", "natural_key": brief["natural_key"]}, "attrs": {}},
        {"from": {"kind": "dashboard_view", "natural_key": dashboard["natural_key"]}, "relation": "derived_from", "to": {"kind": "scorecard", "natural_key": scorecard["natural_key"]}, "attrs": {}},
    ]
    evidence = [{"capture_id": item["capture_id"], "entity": {"kind": "scorecard", "natural_key": scorecard["natural_key"]}, "note": "ops insight evidence"} for item in captures[:4]]
    return {"run_id": run_id, "skill_key": "ops_insight", "status": "normalized", "note": "ops insight bootstrap graph", "entities": entities, "relations": relations, "evidence": evidence}


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a conservative ops_insight graph from shared SQLite captures.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    conn = open_db(args.db)
    print(json.dumps(build_graph(conn, args.run_id), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
