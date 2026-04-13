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
        {
            "capture_id": row[0],
            "collector": row[1],
            "tool": row[2],
            "argv": json.loads(row[3]),
            "stdout": row[4],
            "stderr": row[5],
            "exit_code": row[6],
        }
        for row in rows
    ]


def detect_shape(captures: list[dict]) -> str:
    joined = "\n".join(item["stdout"] for item in captures)
    if "webdav" in joined.lower() or "davfs" in joined.lower():
        return "external_integration"
    return "local_install"


def build_graph(conn: sqlite3.Connection, run_id: str) -> dict:
    captures = load_captures(conn, run_id)
    if not captures:
        raise SystemExit(f"no service_deployment captures found for run_id={run_id}")
    shape = detect_shape(captures)
    apt_present = any("/apt" in item["stdout"] or item["stdout"].strip().endswith("/apt") for item in captures if item["collector"] == "package_managers")
    snap_present = any("/snap" in item["stdout"] or item["stdout"].strip().endswith("/snap") for item in captures if item["collector"] == "package_managers")
    docker_present = any("/docker" in item["stdout"] or item["stdout"].strip().endswith("/docker") for item in captures if item["collector"] == "package_managers")
    preflight = {
        "kind": "deployment_preflight",
        "natural_key": f"deployment_preflight:{run_id}",
        "title": "Deployment preflight",
        "attrs": {"shape": shape, "apt": apt_present, "snap": snap_present, "docker": docker_present},
    }
    plan = {
        "kind": "deployment_plan",
        "natural_key": f"deployment_plan:{run_id}",
        "title": "Deployment plan",
        "attrs": {"shape": shape, "preferred_paths": [name for name, ok in [("apt", apt_present), ("snap", snap_present), ("docker", docker_present)] if ok]},
    }
    result = {
        "kind": "deployment_result",
        "natural_key": f"deployment_result:{run_id}",
        "title": "Deployment bootstrap result",
        "attrs": {"status": "prepared_only", "executed": False},
    }
    entities = [preflight, plan, result]
    relations = [
        {
            "from": {"kind": "deployment_plan", "natural_key": plan["natural_key"]},
            "relation": "derived_from",
            "to": {"kind": "deployment_preflight", "natural_key": preflight["natural_key"]},
            "attrs": {},
        },
        {
            "from": {"kind": "deployment_result", "natural_key": result["natural_key"]},
            "relation": "derived_from",
            "to": {"kind": "deployment_plan", "natural_key": plan["natural_key"]},
            "attrs": {},
        },
    ]
    evidence = [
        {
            "capture_id": item["capture_id"],
            "entity": {"kind": "deployment_preflight", "natural_key": preflight["natural_key"]},
            "note": "deployment preflight evidence",
        }
        for item in captures[:4]
    ]
    return {
        "run_id": run_id,
        "skill_key": "service_deployment",
        "status": "normalized",
        "note": "deployment bootstrap graph",
        "entities": entities,
        "relations": relations,
        "evidence": evidence,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a conservative deployment graph from shared SQLite captures.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    conn = open_db(args.db)
    print(json.dumps(build_graph(conn, args.run_id), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
