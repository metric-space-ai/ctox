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
        raise SystemExit(f"no recovery_assurance captures found for run_id={run_id}")
    coverage = {"kind": "backup_coverage", "natural_key": f"backup_coverage:{run_id}", "title": "Backup coverage", "attrs": {"status": "partial"}}
    restore = {"kind": "restore_evidence", "natural_key": f"restore_evidence:{run_id}", "title": "Restore evidence", "attrs": {"verified": False}}
    rpo_gap = {"kind": "rpo_gap", "natural_key": f"rpo_gap:{run_id}", "title": "RPO gap", "attrs": {"gap": "unknown"}}
    runbook = {"kind": "dr_runbook", "natural_key": f"dr_runbook:{run_id}", "title": "DR runbook note", "attrs": {"note": "Use the observed backup tool and test restore path before trusting recovery."}}
    evidence = []
    for item in captures:
        if item["tool"] in ("restic", "borg", "tar", "pg_restore") and item["exit_code"] == 0:
            restore["attrs"]["verified"] = True
            coverage["attrs"]["status"] = "observed"
            evidence.append({"capture_id": item["capture_id"], "entity": {"kind": "restore_evidence", "natural_key": restore["natural_key"]}, "note": f"{item['tool']} returned success"})
    if restore["attrs"]["verified"]:
        rpo_gap["attrs"]["gap"] = "not_proven_but_tooling_present"
    entities = [coverage, restore, rpo_gap, runbook]
    relations = [
        {"from": {"kind": "restore_evidence", "natural_key": restore["natural_key"]}, "relation": "derived_from", "to": {"kind": "backup_coverage", "natural_key": coverage["natural_key"]}, "attrs": {}},
        {"from": {"kind": "rpo_gap", "natural_key": rpo_gap["natural_key"]}, "relation": "derived_from", "to": {"kind": "backup_coverage", "natural_key": coverage["natural_key"]}, "attrs": {}},
        {"from": {"kind": "dr_runbook", "natural_key": runbook["natural_key"]}, "relation": "derived_from", "to": {"kind": "backup_coverage", "natural_key": coverage["natural_key"]}, "attrs": {}},
    ]
    return {"run_id": run_id, "skill_key": "recovery_assurance", "status": "normalized", "note": "recovery bootstrap graph", "entities": entities, "relations": relations, "evidence": evidence}


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a conservative recovery_assurance graph from shared SQLite captures.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    conn = open_db(args.db)
    print(json.dumps(build_graph(conn, args.run_id), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
