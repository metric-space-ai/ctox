#!/usr/bin/env python3
import argparse
import json
import re
import sqlite3
import sys
from pathlib import Path


HOSTNAME_RE = re.compile(r"Static hostname:\s*(\S+)")
FAILED_UNIT_RE = re.compile(r"^([A-Za-z0-9_.@-]+\.service)\s+loaded\s+failed", re.MULTILINE)


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
    for item in grouped.get(("cpu_memory", "hostnamectl"), []):
        match = HOSTNAME_RE.search(item["stdout"])
        if match:
            return match.group(1)
    return "unknown-host"


def parse_memory_pressure(grouped: dict[tuple[str, str], list[dict]]) -> tuple[list[dict], list[dict]]:
    entities = []
    evidence = []
    for item in grouped.get(("cpu_memory", "free"), []):
        lines = [line.strip() for line in item["stdout"].splitlines() if line.strip()]
        mem_line = next((line for line in lines if line.startswith("Mem:")), None)
        if not mem_line:
            continue
        evidence.append(item)
        parts = mem_line.split()
        if len(parts) < 3:
            continue
        total = int(parts[1])
        used = int(parts[2])
        if total <= 0:
            continue
        usage = used / total
        if usage >= 0.85:
            entities.append(
                {
                    "kind": "resource_pressure",
                    "natural_key": "resource_pressure:memory",
                    "title": "Memory pressure",
                    "attrs": {"resource": "memory", "used_mb": used, "total_mb": total, "usage_ratio": usage},
                }
            )
    return entities, evidence


def parse_disk_pressure(grouped: dict[tuple[str, str], list[dict]]) -> tuple[list[dict], list[dict]]:
    entities = []
    evidence = []
    for item in grouped.get(("disk_io", "df"), []):
        evidence.append(item)
        for line in item["stdout"].splitlines()[1:]:
            parts = line.split()
            if len(parts) < 6:
                continue
            use_pct = parts[4]
            mount = parts[5]
            if not use_pct.endswith("%"):
                continue
            pct = int(use_pct[:-1])
            if pct >= 90:
                entities.append(
                    {
                        "kind": "resource_pressure",
                        "natural_key": f"resource_pressure:disk:{mount}",
                        "title": f"Disk pressure on {mount}",
                        "attrs": {"resource": "disk", "mount": mount, "usage_percent": pct},
                    }
                )
    return entities, evidence


def parse_failed_units(grouped: dict[tuple[str, str], list[dict]]) -> tuple[list[dict], list[dict], list[dict]]:
    units = []
    anomalies = []
    evidence = []
    for item in grouped.get(("service_status", "systemctl"), []):
        if "--failed" not in item["argv"]:
            continue
        evidence.append(item)
        for unit_id in FAILED_UNIT_RE.findall(item["stdout"]):
            units.append(
                {
                    "kind": "service_health",
                    "natural_key": f"service_health:{unit_id}",
                    "title": unit_id,
                    "attrs": {"unit": unit_id, "status": "failed"},
                }
            )
            anomalies.append(
                {
                    "kind": "anomaly",
                    "natural_key": f"anomaly:failed_unit:{unit_id}",
                    "title": f"Failed unit: {unit_id}",
                    "attrs": {"category": "failed_unit", "unit": unit_id},
                }
            )
    return units, anomalies, evidence


def parse_endpoint(grouped: dict[tuple[str, str], list[dict]]) -> tuple[list[dict], list[dict], list[dict]]:
    checks = []
    anomalies = []
    evidence = []
    for item in grouped.get(("endpoint_probe", "curl"), []):
        evidence.append(item)
        line = item["stdout"].strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) != 2:
            continue
        status_code = parts[0]
        total_time = float(parts[1])
        url = item["argv"][-1]
        checks.append(
            {
                "kind": "endpoint_check",
                "natural_key": f"endpoint_check:{url}",
                "title": url,
                "attrs": {"url": url, "status_code": status_code, "time_total_sec": total_time},
            }
        )
        if not status_code.startswith(("2", "3")):
            anomalies.append(
                {
                    "kind": "anomaly",
                    "natural_key": f"anomaly:endpoint:{url}",
                    "title": f"Endpoint returned {status_code}",
                    "attrs": {"category": "endpoint_status", "url": url, "status_code": status_code},
                }
            )
    return checks, anomalies, evidence


def parse_journal_anomalies(grouped: dict[tuple[str, str], list[dict]]) -> tuple[list[dict], list[dict]]:
    findings = []
    evidence = []
    for item in grouped.get(("service_logs", "journalctl"), []):
        evidence.append(item)
        for line in item["stdout"].splitlines():
            line = line.strip()
            if "failed" in line.lower() or "error" in line.lower() or "timed out" in line.lower():
                findings.append(
                    {
                        "kind": "anomaly",
                        "natural_key": f"anomaly:journal:{abs(hash(line))}",
                        "title": "Journal warning",
                        "attrs": {"category": "journal", "sample": line[:200]},
                    }
                )
    return findings[:5], evidence


def build_graph(conn: sqlite3.Connection, run_id: str) -> dict:
    captures = load_captures(conn, run_id)
    if not captures:
        raise SystemExit(f"no reliability captures found for run_id={run_id}")
    grouped = group_captures(captures)
    host_name = parse_hostname(grouped)
    host = {"kind": "host", "natural_key": f"host:{host_name}", "title": host_name, "attrs": {"hostname": host_name}}

    pressure_entities, pressure_evidence = parse_memory_pressure(grouped)
    disk_entities, disk_evidence = parse_disk_pressure(grouped)
    service_entities, failed_anomalies, service_evidence = parse_failed_units(grouped)
    endpoint_entities, endpoint_anomalies, endpoint_evidence = parse_endpoint(grouped)
    journal_anomalies, journal_evidence = parse_journal_anomalies(grouped)
    anomalies = failed_anomalies + endpoint_anomalies + journal_anomalies

    assessment_status = "healthy" if not (pressure_entities or disk_entities or anomalies) else "degraded"
    assessment = {
        "kind": "health_assessment",
        "natural_key": f"health_assessment:{host_name}",
        "title": f"Health assessment for {host_name}",
        "attrs": {"status": assessment_status, "anomaly_count": len(anomalies)},
    }

    entities = [host, assessment]
    entities.extend(pressure_entities)
    entities.extend(disk_entities)
    entities.extend(service_entities)
    entities.extend(endpoint_entities)
    entities.extend(anomalies)

    remediation = []
    if pressure_entities:
        remediation.append(
            {
                "kind": "remediation_suggestion",
                "natural_key": "remediation_suggestion:memory",
                "title": "Inspect top memory consumers",
                "attrs": {"action": "inspect_memory_consumers", "risk": "low"},
            }
        )
    if failed_anomalies:
        remediation.append(
            {
                "kind": "remediation_suggestion",
                "natural_key": "remediation_suggestion:failed_units",
                "title": "Inspect failed services with systemctl status and journalctl -u",
                "attrs": {"action": "inspect_failed_units", "risk": "low"},
            }
        )
    entities.extend(remediation)

    relations = [
        {
            "from": {"kind": "health_assessment", "natural_key": assessment["natural_key"]},
            "relation": "assesses",
            "to": {"kind": "host", "natural_key": host["natural_key"]},
            "attrs": {},
        }
    ]
    for entity in pressure_entities + disk_entities:
        relations.append(
            {
                "from": {"kind": entity["kind"], "natural_key": entity["natural_key"]},
                "relation": "observed_on",
                "to": {"kind": "host", "natural_key": host["natural_key"]},
                "attrs": {},
            }
        )
    for entity in anomalies:
        target = {"kind": "host", "natural_key": host["natural_key"]}
        if entity["attrs"].get("unit"):
            unit_key = f"systemd_unit:{entity['attrs']['unit']}"
            entities.append({"kind": "systemd_unit", "natural_key": unit_key, "title": entity["attrs"]["unit"], "attrs": {}})
            target = {"kind": "systemd_unit", "natural_key": unit_key}
        relations.append(
            {
                "from": {"kind": "anomaly", "natural_key": entity["natural_key"]},
                "relation": "affects",
                "to": target,
                "attrs": {},
            }
        )
    for entity in remediation:
        relations.append(
            {
                "from": {"kind": "remediation_suggestion", "natural_key": entity["natural_key"]},
                "relation": "suggests",
                "to": {"kind": "health_assessment", "natural_key": assessment["natural_key"]},
                "attrs": {},
            }
        )

    evidence = []
    evidence_sources = pressure_evidence + disk_evidence + service_evidence + endpoint_evidence + journal_evidence
    for entity in [assessment] + pressure_entities + disk_entities + service_entities + endpoint_entities + anomalies + remediation:
        for item in evidence_sources:
            evidence.append(
                {
                    "capture_id": item["capture_id"],
                    "entity": {"kind": entity["kind"], "natural_key": entity["natural_key"]},
                    "note": "Derived from reliability capture output.",
                }
            )

    return {
        "run_id": run_id,
        "skill_key": "reliability_ops",
        "status": "normalized",
        "full_sweep": False,
        "note": "Conservative bootstrap reliability assessment from stored captures.",
        "entities": entities,
        "relations": relations,
        "evidence": evidence,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Bootstrap a conservative reliability graph from stored captures.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    conn = open_db(args.db)
    print(json.dumps(build_graph(conn, args.run_id), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
