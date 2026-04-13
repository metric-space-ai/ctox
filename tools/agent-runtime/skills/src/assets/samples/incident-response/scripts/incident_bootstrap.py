#!/usr/bin/env python3
import argparse
import json
import re
import sqlite3
import sys
from pathlib import Path


HOSTNAME_RE = re.compile(r"Static hostname:\s*(\S+)")
FAILED_RE = re.compile(r"\bfailed\b", re.IGNORECASE)


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


def group_captures(captures: list[dict]) -> dict[tuple[str, str], list[dict]]:
    grouped: dict[tuple[str, str], list[dict]] = {}
    for item in captures:
        grouped.setdefault((item["collector"], item["tool"]), []).append(item)
    return grouped


def parse_hostname(grouped: dict[tuple[str, str], list[dict]]) -> str:
    for item in grouped.get(("incident_overview", "hostnamectl"), []):
        match = HOSTNAME_RE.search(item["stdout"])
        if match:
            return match.group(1)
    return "unknown-host"


def build_graph(conn: sqlite3.Connection, run_id: str) -> dict:
    captures = load_captures(conn, run_id)
    if not captures:
        raise SystemExit(f"no incident captures found for run_id={run_id}")
    grouped = group_captures(captures)
    host_name = parse_hostname(grouped)
    host = {"kind": "host", "natural_key": f"host:{host_name}", "title": host_name, "attrs": {"hostname": host_name}}
    incident = {
        "kind": "incident_case",
        "natural_key": f"incident_case:{run_id}",
        "title": f"Incident on {host_name}",
        "attrs": {"status": "open", "run_id": run_id},
    }
    anomalies: list[dict] = []
    evidence: list[dict] = []
    hypotheses: list[str] = []

    for item in grouped.get(("incident_service", "systemctl"), []):
        if item["stdout"] and FAILED_RE.search(item["stdout"]):
            unit = next((token for token in item["argv"] if token.endswith(".service")), "unknown.service")
            anomalies.append({"kind": "anomaly", "natural_key": f"anomaly:failed_service:{unit}", "title": f"Failed service {unit}", "attrs": {"category": "failed_service", "unit": unit}})
            hypotheses.append(f"{unit} is failed or crash-looping")
            evidence.append({"capture_id": item["capture_id"], "entity": {"kind": "anomaly", "natural_key": f"anomaly:failed_service:{unit}"}, "note": "systemctl status showed a failed service"})
    for item in grouped.get(("dependency_probe", "curl"), []):
        head = item["stdout"].splitlines()[0] if item["stdout"].splitlines() else ""
        if "200" not in head and "301" not in head and "302" not in head:
            url = item["argv"][-1]
            anomalies.append({"kind": "anomaly", "natural_key": f"anomaly:endpoint:{url}", "title": f"Endpoint degraded {url}", "attrs": {"category": "endpoint", "url": url, "sample": head}})
            hypotheses.append(f"Downstream endpoint {url} is unhealthy")
            evidence.append({"capture_id": item["capture_id"], "entity": {"kind": "anomaly", "natural_key": f"anomaly:endpoint:{url}"}, "note": "curl probe returned a non-healthy response"})
    for item in grouped.get(("incident_logs", "journalctl"), []):
        lines = [line.strip() for line in item["stdout"].splitlines() if line.strip()]
        bad = [line for line in lines if any(word in line.lower() for word in ("failed", "error", "timed out"))]
        for index, line in enumerate(bad[:3], start=1):
            natural_key = f"anomaly:journal:{run_id}:{index}"
            anomalies.append({"kind": "anomaly", "natural_key": natural_key, "title": "Journal incident signal", "attrs": {"category": "journal", "sample": line[:220]}})
            evidence.append({"capture_id": item["capture_id"], "entity": {"kind": "anomaly", "natural_key": natural_key}, "note": line[:220]})
        if bad:
            hypotheses.append("Recent journal warnings suggest a local service or dependency failure")

    hypothesis_entity = {
        "kind": "hypothesis_set",
        "natural_key": f"hypothesis_set:{run_id}",
        "title": f"Hypotheses for {run_id}",
        "attrs": {"hypotheses": hypotheses[:3] or ["Insufficient evidence; gather a narrower service-specific slice"]},
    }
    mitigation = {
        "kind": "mitigation_action",
        "natural_key": f"mitigation_action:{run_id}",
        "title": "Suggested mitigation",
        "attrs": {"action": "inspect_narrow_scope_first", "risk": "low", "note": "Inspect the failed unit or endpoint before any restart or rollback"},
    }
    status = {
        "kind": "status_update",
        "natural_key": f"status_update:{run_id}",
        "title": f"Incident status for {run_id}",
        "attrs": {"severity": "sev3", "summary": f"{len(anomalies)} anomaly signals observed", "open": True},
    }
    entities = [host, incident, hypothesis_entity, mitigation, status, *anomalies]
    relations = [
        {"from": {"kind": "incident_case", "natural_key": incident["natural_key"]}, "relation": "affects", "to": {"kind": "host", "natural_key": host["natural_key"]}, "attrs": {}},
        {"from": {"kind": "hypothesis_set", "natural_key": hypothesis_entity["natural_key"]}, "relation": "derived_from", "to": {"kind": "incident_case", "natural_key": incident["natural_key"]}, "attrs": {}},
        {"from": {"kind": "mitigation_action", "natural_key": mitigation["natural_key"]}, "relation": "suggests", "to": {"kind": "incident_case", "natural_key": incident["natural_key"]}, "attrs": {}},
        {"from": {"kind": "status_update", "natural_key": status["natural_key"]}, "relation": "derived_from", "to": {"kind": "incident_case", "natural_key": incident["natural_key"]}, "attrs": {}},
    ]
    for anomaly in anomalies:
        relations.append({"from": {"kind": "anomaly", "natural_key": anomaly["natural_key"]}, "relation": "affects", "to": {"kind": "incident_case", "natural_key": incident["natural_key"]}, "attrs": {}})
    return {
        "run_id": run_id,
        "skill_key": "incident_response",
        "status": "normalized",
        "note": "incident bootstrap graph",
        "entities": entities,
        "relations": relations,
        "evidence": evidence,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Build a conservative incident_response graph from shared SQLite captures.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    conn = open_db(args.db)
    print(json.dumps(build_graph(conn, args.run_id), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
