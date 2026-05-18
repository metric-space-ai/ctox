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
        raise SystemExit(f"no automation_engineering captures found for run_id={run_id}")
    queue_mentions = 0
    schedule_mentions = 0
    for item in captures:
        queue_mentions += item["stdout"].lower().count("queue")
        schedule_mentions += item["stdout"].lower().count("schedule")
    pattern = {"kind": "task_pattern", "natural_key": f"task_pattern:{run_id}", "title": "Observed ops repetition", "attrs": {"queue_mentions": queue_mentions, "schedule_mentions": schedule_mentions}}
    recipe = {"kind": "automation_recipe", "natural_key": f"automation_recipe:{run_id}", "title": "Automation recipe", "attrs": {"shape": "repo_script_plus_ctox_schedule", "dry_run_first": True}}
    workflow = {"kind": "workflow_version", "natural_key": f"workflow_version:{run_id}", "title": "Workflow version", "attrs": {"version": 1, "status": "draft"}}
    test_evidence = {"kind": "test_evidence", "natural_key": f"test_evidence:{run_id}", "title": "Automation test evidence", "attrs": {"verified": False, "note": "bootstrap only"}}
    note = {"kind": "adoption_note", "natural_key": f"adoption_note:{run_id}", "title": "Adoption note", "attrs": {"note": "Keep recurring work inside ctox schedule and queue, not a hidden daemon."}}
    entities = [pattern, recipe, workflow, test_evidence, note]
    relations = [
        {"from": {"kind": "automation_recipe", "natural_key": recipe["natural_key"]}, "relation": "derived_from", "to": {"kind": "task_pattern", "natural_key": pattern["natural_key"]}, "attrs": {}},
        {"from": {"kind": "workflow_version", "natural_key": workflow["natural_key"]}, "relation": "derived_from", "to": {"kind": "automation_recipe", "natural_key": recipe["natural_key"]}, "attrs": {}},
        {"from": {"kind": "test_evidence", "natural_key": test_evidence["natural_key"]}, "relation": "derived_from", "to": {"kind": "workflow_version", "natural_key": workflow["natural_key"]}, "attrs": {}},
        {"from": {"kind": "adoption_note", "natural_key": note["natural_key"]}, "relation": "derived_from", "to": {"kind": "automation_recipe", "natural_key": recipe["natural_key"]}, "attrs": {}},
    ]
    evidence = [{"capture_id": item["capture_id"], "entity": {"kind": "task_pattern", "natural_key": pattern["natural_key"]}, "note": "automation substrate evidence"} for item in captures[:4]]
    return {"run_id": run_id, "skill_key": "automation_engineering", "status": "normalized", "note": "automation bootstrap graph", "entities": entities, "relations": relations, "evidence": evidence}


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a conservative automation_engineering graph from shared SQLite captures.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    conn = open_db(args.db)
    print(json.dumps(build_graph(conn, args.run_id), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
