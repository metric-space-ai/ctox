#!/usr/bin/env python3
import argparse
import json
import sqlite3
import sys
from pathlib import Path


def open_db(db_path: str) -> sqlite3.Connection:
    return sqlite3.connect(Path(db_path))


def summary(conn: sqlite3.Connection, skill_key: str | None = None) -> dict:
    run_where = ""
    entity_join = ""
    relation_join = ""
    coverage_where = ""
    params: list[str] = []
    if skill_key:
        run_where = "WHERE skill_key = ?"
        entity_join = "JOIN discovery_run r ON r.run_id = e.last_run_id WHERE r.skill_key = ?"
        relation_join = "JOIN discovery_run r ON r.run_id = rel.last_run_id WHERE r.skill_key = ?"
        coverage_where = "AND r.skill_key = ?"
        params = [skill_key]
    runs = [
        {
            "run_id": row[0],
            "skill_key": row[1],
            "status": row[2],
            "started_at": row[3],
            "finished_at": row[4],
        }
        for row in conn.execute(
            f"""
            SELECT run_id, skill_key, status, started_at, finished_at
            FROM discovery_run
            {run_where}
            ORDER BY started_at DESC
            """,
            params,
        )
    ]
    entities = [
        {
            "kind": row[0],
            "active": row[1],
            "inactive": row[2],
        }
        for row in conn.execute(
            f"""
            SELECT
                e.kind,
                SUM(CASE WHEN is_active = 1 THEN 1 ELSE 0 END) AS active_count,
                SUM(CASE WHEN is_active = 0 THEN 1 ELSE 0 END) AS inactive_count
            FROM discovery_entity e
            {entity_join}
            GROUP BY e.kind
            ORDER BY e.kind
            """,
            params,
        )
    ]
    relations = [
        {
            "relation": row[0],
            "active": row[1],
            "inactive": row[2],
        }
        for row in conn.execute(
            f"""
            SELECT
                rel.relation,
                SUM(CASE WHEN is_active = 1 THEN 1 ELSE 0 END) AS active_count,
                SUM(CASE WHEN is_active = 0 THEN 1 ELSE 0 END) AS inactive_count
            FROM discovery_relation rel
            {relation_join}
            GROUP BY rel.relation
            ORDER BY rel.relation
            """,
            params,
        )
    ]
    coverage_gaps = [
        {
            "natural_key": row[0],
            "title": row[1],
            "attrs": json.loads(row[2]),
        }
        for row in conn.execute(
            f"""
            SELECT e.natural_key, e.title, e.attrs_json
            FROM discovery_entity e
            {"JOIN discovery_run r ON r.run_id = e.last_run_id" if skill_key else ""}
            WHERE e.kind = 'coverage_gap' AND e.is_active = 1
            {coverage_where}
            ORDER BY e.natural_key
            """,
            params,
        )
    ]
    return {
        "runs": runs,
        "entities": entities,
        "relations": relations,
        "active_coverage_gaps": coverage_gaps,
    }


def export_cytoscape(conn: sqlite3.Connection, skill_key: str | None = None) -> dict:
    node_query = """
        SELECT e.entity_id, e.kind, e.natural_key, e.title, e.attrs_json, e.is_active
        FROM discovery_entity e
    """
    edge_query = """
        SELECT rel.relation_id, rel.from_entity_id, rel.relation, rel.to_entity_id, rel.attrs_json, rel.is_active
        FROM discovery_relation rel
    """
    params: list[str] = []
    if skill_key:
        node_query += " JOIN discovery_run r ON r.run_id = e.last_run_id WHERE r.skill_key = ?"
        edge_query += " JOIN discovery_run r ON r.run_id = rel.last_run_id WHERE r.skill_key = ?"
        params = [skill_key]
    node_query += " ORDER BY e.kind, e.natural_key"
    edge_query += " ORDER BY rel.relation_id"
    node_rows = conn.execute(node_query, params).fetchall()
    edge_rows = conn.execute(edge_query, params).fetchall()
    return {
        "nodes": [
            {
                "data": {
                    "id": row[0],
                    "kind": row[1],
                    "natural_key": row[2],
                    "label": row[3],
                    "attrs": json.loads(row[4]),
                    "is_active": bool(row[5]),
                }
            }
            for row in node_rows
        ],
        "edges": [
            {
                "data": {
                    "id": row[0],
                    "source": row[1],
                    "target": row[3],
                    "relation": row[2],
                    "attrs": json.loads(row[4]),
                    "is_active": bool(row[5]),
                }
            }
            for row in edge_rows
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Query or export discovery graph state.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    summary_parser = subparsers.add_parser("summary")
    summary_parser.add_argument("--db", required=True)
    summary_parser.add_argument("--skill-key")

    export_parser = subparsers.add_parser("export-cytoscape")
    export_parser.add_argument("--db", required=True)
    export_parser.add_argument("--skill-key")

    args = parser.parse_args()
    conn = open_db(args.db)
    if args.command == "summary":
        print(json.dumps(summary(conn, args.skill_key), indent=2))
        return 0
    print(json.dumps(export_cytoscape(conn, args.skill_key), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
