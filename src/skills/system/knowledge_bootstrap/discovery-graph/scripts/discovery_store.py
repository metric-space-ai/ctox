#!/usr/bin/env python3
import argparse
import hashlib
import json
import sqlite3
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Optional


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


SCHEMA = """
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS discovery_run (
    run_id TEXT PRIMARY KEY,
    skill_key TEXT NOT NULL DEFAULT 'discovery_graph',
    scope_json TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS discovery_capture (
    capture_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES discovery_run(run_id) ON DELETE CASCADE,
    collector TEXT NOT NULL,
    tool TEXT NOT NULL,
    target TEXT NOT NULL,
    command_json TEXT NOT NULL,
    stdout_text TEXT NOT NULL,
    stderr_text TEXT NOT NULL,
    exit_code INTEGER,
    captured_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_discovery_capture_run ON discovery_capture(run_id, collector, captured_at);

CREATE TABLE IF NOT EXISTS discovery_entity (
    entity_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    natural_key TEXT NOT NULL,
    title TEXT NOT NULL,
    attrs_json TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    last_run_id TEXT NOT NULL REFERENCES discovery_run(run_id),
    is_active INTEGER NOT NULL DEFAULT 1,
    UNIQUE(kind, natural_key)
);

CREATE TABLE IF NOT EXISTS discovery_relation (
    relation_id TEXT PRIMARY KEY,
    from_entity_id TEXT NOT NULL REFERENCES discovery_entity(entity_id) ON DELETE CASCADE,
    relation TEXT NOT NULL,
    to_entity_id TEXT NOT NULL REFERENCES discovery_entity(entity_id) ON DELETE CASCADE,
    attrs_json TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    last_run_id TEXT NOT NULL REFERENCES discovery_run(run_id),
    is_active INTEGER NOT NULL DEFAULT 1,
    UNIQUE(from_entity_id, relation, to_entity_id)
);

CREATE TABLE IF NOT EXISTS discovery_evidence (
    evidence_id TEXT PRIMARY KEY,
    capture_id TEXT NOT NULL REFERENCES discovery_capture(capture_id) ON DELETE CASCADE,
    entity_id TEXT REFERENCES discovery_entity(entity_id) ON DELETE CASCADE,
    relation_id TEXT REFERENCES discovery_relation(relation_id) ON DELETE CASCADE,
    note TEXT NOT NULL,
    created_at TEXT NOT NULL
);
"""


def open_db(db_path: str) -> sqlite3.Connection:
    path = Path(db_path)
    path.parent.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(path)
    conn.execute("PRAGMA foreign_keys=ON")
    conn.executescript(SCHEMA)
    ensure_column(
        conn,
        "discovery_run",
        "skill_key",
        "TEXT NOT NULL DEFAULT 'discovery_graph'",
    )
    return conn


def ensure_column(
    conn: sqlite3.Connection,
    table: str,
    column: str,
    ddl: str,
) -> None:
    columns = {
        row[1]
        for row in conn.execute(f"PRAGMA table_info({table})").fetchall()
    }
    if column in columns:
        return
    conn.execute(f"ALTER TABLE {table} ADD COLUMN {column} {ddl}")


def stable_id(prefix: str, payload: str) -> str:
    digest = hashlib.sha256(payload.encode("utf-8")).hexdigest()[:16]
    return f"{prefix}-{digest}"


def load_json(input_path: str) -> Dict[str, Any]:
    if input_path == "-":
        return json.load(sys.stdin)
    with open(input_path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def ensure_run(
    conn: sqlite3.Connection,
    run_id: str,
    skill_key: Optional[str] = None,
    scope_json: Optional[str] = None,
    started_at: Optional[str] = None,
    finished_at: Optional[str] = None,
    status: Optional[str] = None,
    note: Optional[str] = None,
) -> None:
    now = now_iso()
    effective_started_at = started_at or now
    effective_finished_at = finished_at or effective_started_at
    conn.execute(
        """
        INSERT INTO discovery_run (run_id, skill_key, scope_json, started_at, finished_at, status, note)
        VALUES (?, ?, ?, ?, ?, 'open', '')
        ON CONFLICT(run_id) DO NOTHING
        """,
        (run_id, skill_key or "discovery_graph", scope_json or "{}", effective_started_at, effective_finished_at),
    )
    updates = []
    values = []
    if skill_key is not None:
        updates.append("skill_key = ?")
        values.append(skill_key)
    if scope_json is not None:
        updates.append("scope_json = ?")
        values.append(scope_json)
    if started_at is not None:
        updates.append("started_at = CASE WHEN started_at > ? THEN ? ELSE started_at END")
        values.extend([started_at, started_at])
    if finished_at is not None:
        updates.append("finished_at = ?")
        values.append(finished_at)
    if status is not None:
        updates.append("status = ?")
        values.append(status)
    if note is not None:
        updates.append("note = ?")
        values.append(note)
    if updates:
        values.append(run_id)
        conn.execute(
            f"UPDATE discovery_run SET {', '.join(updates)} WHERE run_id = ?",
            values,
        )


def capture_time_bounds(captures: list[dict[str, Any]]) -> tuple[Optional[str], Optional[str]]:
    started_candidates = [
        item.get("started_at") for item in captures if item.get("started_at")
    ]
    finished_candidates = [
        item.get("finished_at") for item in captures if item.get("finished_at")
    ]
    started_at = min(started_candidates) if started_candidates else None
    finished_at = max(finished_candidates) if finished_candidates else None
    return started_at, finished_at


def store_capture(
    conn: sqlite3.Connection,
    payload: Dict[str, Any],
    target: Optional[str],
) -> Dict[str, Any]:
    skill_key = payload.get("skill_key") or "discovery_graph"
    if "collectors" in payload:
        shared_run_id = payload.get("run_id") or stable_id(
            "run",
            json.dumps(
                {
                    "repo_root": payload.get("repo_root"),
                    "captured_at": payload.get("captured_at"),
                },
                sort_keys=True,
            ),
        )
        collector_started = []
        collector_finished = []
        for collector_payload in payload.get("collectors", []):
            started_at, finished_at = capture_time_bounds(collector_payload.get("captures", []))
            if started_at:
                collector_started.append(started_at)
            if finished_at:
                collector_finished.append(finished_at)
        started_at = min(collector_started) if collector_started else payload.get("captured_at")
        finished_at = max(collector_finished) if collector_finished else payload.get("captured_at")
        ensure_run(
            conn,
            shared_run_id,
            skill_key=skill_key,
            scope_json=json.dumps(
                {"repo_root": payload.get("repo_root"), "target": target or "local"},
                sort_keys=True,
            ),
            started_at=started_at,
            finished_at=finished_at,
            status="capturing",
        )
        run_ids = []
        capture_ids = []
        for collector_payload in payload.get("collectors", []):
            collector_payload = dict(collector_payload)
            collector_payload["run_id"] = shared_run_id
            collector_payload["skill_key"] = skill_key
            result = store_capture(conn, collector_payload, target)
            run_ids.append(result["run_id"])
            capture_ids.extend(result["capture_ids"])
        ensure_run(
            conn,
            shared_run_id,
            skill_key=skill_key,
            finished_at=finished_at or now_iso(),
            status="captured",
        )
        conn.commit()
        return {"run_id": shared_run_id, "capture_ids": capture_ids, "skill_key": skill_key}
    run_id = payload.get("run_id") or stable_id(
        "run",
        json.dumps(
            {
                "collector": payload.get("collector"),
                "captured_at": payload.get("captured_at"),
                "repo_root": payload.get("repo_root"),
            },
            sort_keys=True,
        ),
    )
    captures = payload.get("captures", [])
    started_at, finished_at = capture_time_bounds(captures)
    ensure_run(
        conn,
        run_id,
        skill_key=skill_key,
        scope_json=json.dumps(
            {
                "collector": payload.get("collector"),
                "repo_root": payload.get("repo_root"),
                "target": target or payload.get("repo_root") or "local",
            },
            sort_keys=True,
        ),
        started_at=started_at or payload.get("captured_at"),
        finished_at=finished_at or payload.get("captured_at"),
        status="capturing",
    )
    stored = []
    collector = payload.get("collector", "unknown")
    effective_target = target or payload.get("repo_root") or "local"
    for item in captures:
        capture_id = stable_id(
            "capture",
            json.dumps(
                {
                    "run_id": run_id,
                    "collector": collector,
                    "tool": item.get("tool"),
                    "argv": item.get("argv"),
                    "started_at": item.get("started_at"),
                },
                sort_keys=True,
            ),
        )
        conn.execute(
            """
            INSERT OR REPLACE INTO discovery_capture (
                capture_id, run_id, collector, tool, target, command_json,
                stdout_text, stderr_text, exit_code, captured_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                capture_id,
                run_id,
                collector,
                item.get("tool", "unknown"),
                effective_target,
                json.dumps(item.get("argv", []), sort_keys=True),
                item.get("stdout", ""),
                item.get("stderr", ""),
                item.get("exit_code"),
                item.get("finished_at") or payload.get("captured_at") or now_iso(),
            ),
        )
        upsert_capture_gap(
            conn,
            run_id,
            capture_id,
            collector,
            effective_target,
            item,
        )
        stored.append(capture_id)
    ensure_run(
        conn,
        run_id,
        skill_key=skill_key,
        finished_at=finished_at or payload.get("captured_at") or now_iso(),
        status="captured",
    )
    conn.commit()
    return {"run_id": run_id, "capture_ids": stored, "skill_key": skill_key}


def upsert_entity(conn: sqlite3.Connection, run_id: str, entity: Dict[str, Any]) -> str:
    kind = entity["kind"]
    natural_key = entity["natural_key"]
    title = entity["title"]
    attrs_json = json.dumps(entity.get("attrs", {}), sort_keys=True)
    now = now_iso()
    existing = conn.execute(
        "SELECT entity_id, first_seen_at FROM discovery_entity WHERE kind = ? AND natural_key = ?",
        (kind, natural_key),
    ).fetchone()
    if existing:
        entity_id, first_seen_at = existing
        conn.execute(
            """
            UPDATE discovery_entity
            SET title = ?, attrs_json = ?, last_seen_at = ?, last_run_id = ?, is_active = 1
            WHERE entity_id = ?
            """,
            (title, attrs_json, now, run_id, entity_id),
        )
        return entity_id
    entity_id = stable_id("entity", f"{kind}:{natural_key}")
    conn.execute(
        """
        INSERT INTO discovery_entity (
            entity_id, kind, natural_key, title, attrs_json,
            first_seen_at, last_seen_at, last_run_id, is_active
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1)
        """,
        (entity_id, kind, natural_key, title, attrs_json, now, now, run_id),
    )
    return entity_id


def upsert_capture_gap(
    conn: sqlite3.Connection,
    run_id: str,
    capture_id: str,
    collector: str,
    target: str,
    item: Dict[str, Any],
) -> Optional[str]:
    tool = item.get("tool") or "unknown"
    natural_key = f"coverage_gap:{target}:{collector}:{tool}"
    if item.get("available", True) and item.get("exit_code") in (0, None):
        row = conn.execute(
            "SELECT entity_id FROM discovery_entity WHERE kind = 'coverage_gap' AND natural_key = ?",
            (natural_key,),
        ).fetchone()
        if row:
            conn.execute(
                "UPDATE discovery_entity SET is_active = 0 WHERE entity_id = ?",
                (row[0],),
            )
        return None

    note = item.get("stderr") or "collector unavailable or failed"
    entity_id = upsert_entity(
        conn,
        run_id,
        {
            "kind": "coverage_gap",
            "natural_key": natural_key,
            "title": f"{collector}:{tool} unavailable",
            "attrs": {
                "collector": collector,
                "tool": tool,
                "target": target,
                "argv": item.get("argv", []),
                "exit_code": item.get("exit_code"),
                "stderr": note,
            },
        },
    )
    evidence_id = stable_id(
        "evidence",
        json.dumps(
            {
                "capture_id": capture_id,
                "entity_id": entity_id,
                "relation_id": None,
                "note": note,
            },
            sort_keys=True,
        ),
    )
    conn.execute(
        """
        INSERT OR REPLACE INTO discovery_evidence (
            evidence_id, capture_id, entity_id, relation_id, note, created_at
        ) VALUES (?, ?, ?, ?, ?, ?)
        """,
        (evidence_id, capture_id, entity_id, None, note, now_iso()),
    )
    return entity_id


def mark_missing_inactive(
    conn: sqlite3.Connection,
    table: str,
    id_column: str,
    seen_ids: list[str],
    skill_key: str,
) -> None:
    skill_filter = "last_run_id IN (SELECT run_id FROM discovery_run WHERE skill_key = ?)"
    if seen_ids:
        placeholders = ", ".join("?" for _ in seen_ids)
        conn.execute(
            f"""
            UPDATE {table}
            SET is_active = 0
            WHERE {skill_filter} AND {id_column} NOT IN ({placeholders})
            """,
            [skill_key, *seen_ids],
        )
    else:
        conn.execute(
            f"UPDATE {table} SET is_active = 0 WHERE {skill_filter}",
            (skill_key,),
        )


def store_graph(conn: sqlite3.Connection, payload: Dict[str, Any]) -> Dict[str, Any]:
    run_id = payload["run_id"]
    ensure_run(
        conn,
        run_id,
        skill_key=payload.get("skill_key") or "discovery_graph",
        finished_at=payload.get("finished_at") or now_iso(),
        status=payload.get("status") or "normalized",
        note=payload.get("note"),
    )
    entity_ids: dict[tuple[str, str], str] = {}
    created_entities = []
    created_relations = []
    for entity in payload.get("entities", []):
        entity_id = upsert_entity(conn, run_id, entity)
        entity_ids[(entity["kind"], entity["natural_key"])] = entity_id
        created_entities.append(entity_id)
    for relation in payload.get("relations", []):
        from_ref = relation["from"]
        to_ref = relation["to"]
        from_id = entity_ids.get((from_ref["kind"], from_ref["natural_key"]))
        if from_id is None:
            from_id = upsert_entity(
                conn,
                run_id,
                {
                    "kind": from_ref["kind"],
                    "natural_key": from_ref["natural_key"],
                    "title": from_ref["natural_key"],
                    "attrs": {},
                },
            )
            entity_ids[(from_ref["kind"], from_ref["natural_key"])] = from_id
        to_id = entity_ids.get((to_ref["kind"], to_ref["natural_key"]))
        if to_id is None:
            to_id = upsert_entity(
                conn,
                run_id,
                {
                    "kind": to_ref["kind"],
                    "natural_key": to_ref["natural_key"],
                    "title": to_ref["natural_key"],
                    "attrs": {},
                },
            )
            entity_ids[(to_ref["kind"], to_ref["natural_key"])] = to_id
        attrs_json = json.dumps(relation.get("attrs", {}), sort_keys=True)
        relation_id = stable_id("relation", f"{from_id}:{relation['relation']}:{to_id}")
        now = now_iso()
        existing = conn.execute(
            """
            SELECT relation_id, first_seen_at
            FROM discovery_relation
            WHERE from_entity_id = ? AND relation = ? AND to_entity_id = ?
            """,
            (from_id, relation["relation"], to_id),
        ).fetchone()
        if existing:
            conn.execute(
                """
                UPDATE discovery_relation
                SET attrs_json = ?, last_seen_at = ?, last_run_id = ?, is_active = 1
                WHERE relation_id = ?
                """,
                (attrs_json, now, run_id, existing[0]),
            )
            relation_id = existing[0]
        else:
            conn.execute(
                """
                INSERT INTO discovery_relation (
                    relation_id, from_entity_id, relation, to_entity_id, attrs_json,
                    first_seen_at, last_seen_at, last_run_id, is_active
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1)
                """,
                (relation_id, from_id, relation["relation"], to_id, attrs_json, now, now, run_id),
            )
        created_relations.append(relation_id)
    for evidence in payload.get("evidence", []):
        entity_id = None
        relation_id = None
        entity_ref = evidence.get("entity")
        relation_ref = evidence.get("relation")
        if entity_ref:
            entity_id = entity_ids.get((entity_ref["kind"], entity_ref["natural_key"]))
        if relation_ref:
            from_ref = relation_ref["from"]
            to_ref = relation_ref["to"]
            from_id = entity_ids.get((from_ref["kind"], from_ref["natural_key"]))
            to_id = entity_ids.get((to_ref["kind"], to_ref["natural_key"]))
            if from_id and to_id:
                row = conn.execute(
                    """
                    SELECT relation_id
                    FROM discovery_relation
                    WHERE from_entity_id = ? AND relation = ? AND to_entity_id = ?
                    """,
                    (from_id, relation_ref["relation"], to_id),
                ).fetchone()
                if row:
                    relation_id = row[0]
        capture_id = evidence["capture_id"]
        note = evidence.get("note", "")
        evidence_id = stable_id(
            "evidence",
            json.dumps(
                {
                    "capture_id": capture_id,
                    "entity_id": entity_id,
                    "relation_id": relation_id,
                    "note": note,
                },
                sort_keys=True,
            ),
        )
        conn.execute(
            """
            INSERT OR REPLACE INTO discovery_evidence (
                evidence_id, capture_id, entity_id, relation_id, note, created_at
            ) VALUES (?, ?, ?, ?, ?, ?)
            """,
            (evidence_id, capture_id, entity_id, relation_id, note, now_iso()),
        )
    if payload.get("full_sweep"):
        skill_key = payload.get("skill_key") or "discovery_graph"
        mark_missing_inactive(
            conn, "discovery_entity", "entity_id", created_entities, skill_key
        )
        mark_missing_inactive(
            conn, "discovery_relation", "relation_id", created_relations, skill_key
        )
    conn.commit()
    return {
        "run_id": run_id,
        "skill_key": payload.get("skill_key") or "discovery_graph",
        "entity_count": len(created_entities),
        "relation_count": len(created_relations),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Initialize and write the discovery graph SQLite store.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    init_parser = subparsers.add_parser("init")
    init_parser.add_argument("--db", required=True)

    capture_parser = subparsers.add_parser("store-capture")
    capture_parser.add_argument("--db", required=True)
    capture_parser.add_argument("--input", required=True, help="JSON file path or - for stdin")
    capture_parser.add_argument("--target")

    graph_parser = subparsers.add_parser("store-graph")
    graph_parser.add_argument("--db", required=True)
    graph_parser.add_argument("--input", required=True, help="JSON file path or - for stdin")

    args = parser.parse_args()
    conn = open_db(args.db)
    if args.command == "init":
        print(json.dumps({"ok": True, "db_path": str(Path(args.db).resolve())}, indent=2))
        return 0
    if args.command == "store-capture":
        payload = load_json(args.input)
        print(json.dumps({"ok": True, **store_capture(conn, payload, args.target)}, indent=2))
        return 0
    payload = load_json(args.input)
    print(json.dumps({"ok": True, **store_graph(conn, payload)}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
