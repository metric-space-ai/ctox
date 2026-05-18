#!/usr/bin/env python3
import argparse
import json
import re
import sqlite3
import sys
from pathlib import Path


HOSTNAME_RE = re.compile(r"Static hostname:\s*(\S+)")
SS_PID_RE = re.compile(r'users:\(\("([^"]+)",pid=(\d+)')
SERVICE_UNIT_RE = re.compile(r"\b([A-Za-z0-9_.@-]+\.service)\b")
CGroup_UNIT_RE = re.compile(r"/([A-Za-z0-9_.@-]+\.service)\b")
JOURNAL_SIGNAL_RE = re.compile(
    r"(permission denied|failed|error|listen|address already in use|no space left)",
    re.IGNORECASE,
)


def open_db(db_path: str) -> sqlite3.Connection:
    return sqlite3.connect(Path(db_path))


def load_captures(conn: sqlite3.Connection, run_id: str) -> list[dict]:
    rows = conn.execute(
        """
        SELECT capture_id, collector, tool, target, command_json, stdout_text, stderr_text, exit_code
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
            "target": row[3],
            "argv": json.loads(row[4]),
            "stdout": row[5],
            "stderr": row[6],
            "exit_code": row[7],
        }
        for row in rows
    ]


def capture_map(captures: list[dict]) -> dict[tuple[str, str], list[dict]]:
    grouped: dict[tuple[str, str], list[dict]] = {}
    for item in captures:
        grouped.setdefault((item["collector"], item["tool"]), []).append(item)
    return grouped


def parse_hostname(grouped: dict[tuple[str, str], list[dict]]) -> tuple[str, list[dict]]:
    for item in grouped.get(("host_identity", "hostnamectl"), []):
        match = HOSTNAME_RE.search(item["stdout"])
        if match:
            return match.group(1), [item]
    for item in grouped.get(("host_identity", "uname"), []):
        parts = item["stdout"].strip().split()
        if len(parts) >= 2:
            return parts[1], [item]
    return "unknown-host", []


def parse_processes(grouped: dict[tuple[str, str], list[dict]]) -> tuple[dict[str, dict], list[dict]]:
    processes: dict[str, dict] = {}
    evidence: list[dict] = []
    for item in grouped.get(("processes", "ps"), []):
        evidence.append(item)
        for line in item["stdout"].splitlines():
            line = line.strip()
            if not line:
                continue
            parts = re.split(r"\s+", line, maxsplit=7)
            if len(parts) < 7:
                continue
            pid, ppid, user, stat, pcpu, pmem, comm = parts[:7]
            args = parts[7] if len(parts) > 7 else comm
            if not pid.isdigit():
                continue
            natural_key = f"process:{pid}"
            processes[pid] = {
                "kind": "process",
                "natural_key": natural_key,
                "title": f"{comm} ({pid})",
                "attrs": {
                    "pid": int(pid),
                    "ppid": int(ppid) if ppid.isdigit() else ppid,
                    "user": user,
                    "stat": stat,
                    "pcpu": pcpu,
                    "pmem": pmem,
                    "comm": comm,
                    "args": args,
                },
            }
    return processes, evidence


def parse_process_cgroups(grouped: dict[tuple[str, str], list[dict]]) -> tuple[dict[str, str], list[dict]]:
    process_to_unit: dict[str, str] = {}
    evidence: list[dict] = []
    for item in grouped.get(("processes", "bash"), []):
        evidence.append(item)
        current_pid = None
        for raw_line in item["stdout"].splitlines():
            line = raw_line.strip()
            if not line:
                current_pid = None
                continue
            if line.startswith("PID="):
                pid = line[4:]
                current_pid = pid if pid.isdigit() else None
                continue
            if not current_pid:
                continue
            match = CGroup_UNIT_RE.search(line)
            if match:
                process_to_unit.setdefault(current_pid, match.group(1))
                current_pid = None
    return process_to_unit, evidence


def parse_listeners(grouped: dict[tuple[str, str], list[dict]]) -> tuple[list[dict], list[tuple[dict, str]], list[dict]]:
    listeners: list[dict] = []
    binds: list[tuple[dict, str]] = []
    evidence: list[dict] = []
    for item in grouped.get(("listeners", "ss"), []):
        evidence.append(item)
        for line in item["stdout"].splitlines():
            line = line.strip()
            if not line:
                continue
            parts = re.split(r"\s+", line, maxsplit=5)
            if len(parts) < 5:
                continue
            proto = parts[0]
            local = parts[4]
            process_name = None
            pid = None
            if len(parts) > 5:
                match = SS_PID_RE.search(parts[5])
                if match:
                    process_name = match.group(1)
                    pid = match.group(2)
            host, port = local.rsplit(":", 1) if ":" in local else (local, "0")
            natural_key = f"listener:{proto}:{local}"
            entity = {
                "kind": "listener",
                "natural_key": natural_key,
                "title": f"{proto}:{local}",
                "attrs": {
                    "proto": proto,
                    "bind": host,
                    "port": port,
                    "pid": int(pid) if pid and pid.isdigit() else None,
                    "process_name": process_name,
                },
            }
            listeners.append(entity)
            if pid and pid.isdigit():
                binds.append((entity, pid))
    return listeners, binds, evidence


def parse_service_blocks(text: str) -> list[dict]:
    records = []
    current: dict[str, str] = {}
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            if current:
                records.append(current)
                current = {}
            continue
        if "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        if current and (key == "Id" or key in current):
            records.append(current)
            current = {}
        current[key] = value
    if current:
        records.append(current)
    return records


def parse_units(grouped: dict[tuple[str, str], list[dict]]) -> tuple[dict[str, dict], dict[str, str], dict[str, str], list[dict]]:
    units: dict[str, dict] = {}
    process_to_unit: dict[str, str] = {}
    unit_to_fragment: dict[str, str] = {}
    evidence: list[dict] = []
    for item in grouped.get(("services", "systemctl"), []):
        argv = item["argv"]
        argv_text = " ".join(argv)
        if "list-units" not in argv_text:
            continue
        evidence.append(item)
        for line in item["stdout"].splitlines():
            parts = re.split(r"\s{2,}|\t+", line.strip())
            if len(parts) < 4:
                continue
            unit_id, load_state, active_state, sub_state = parts[:4]
            if not unit_id.endswith(".service"):
                continue
            units.setdefault(
                unit_id,
                {
                    "kind": "systemd_unit",
                    "natural_key": f"systemd_unit:{unit_id}",
                    "title": unit_id,
                    "attrs": {
                        "load_state": load_state,
                        "active_state": active_state,
                        "sub_state": sub_state,
                    },
                },
            )
    for item in grouped.get(("services", "systemctl"), []):
        argv = item["argv"]
        argv_text = " ".join(argv)
        if "show" not in argv or (
            "show --type=service" not in argv_text and not any(token.endswith(".service") for token in argv)
        ):
            continue
        evidence.append(item)
        for block in parse_service_blocks(item["stdout"]):
            unit_id = block.get("Id")
            if not unit_id:
                continue
            natural_key = f"systemd_unit:{unit_id}"
            units[unit_id] = {
                "kind": "systemd_unit",
                "natural_key": natural_key,
                "title": unit_id,
                "attrs": {
                    "names": block.get("Names"),
                    "load_state": block.get("LoadState"),
                    "active_state": block.get("ActiveState"),
                    "sub_state": block.get("SubState"),
                    "main_pid": int(block["MainPID"]) if block.get("MainPID", "").isdigit() else None,
                    "fragment_path": block.get("FragmentPath"),
                    "description": block.get("Description"),
                },
            }
            if block.get("MainPID", "").isdigit():
                process_to_unit[block["MainPID"]] = unit_id
            if block.get("FragmentPath"):
                unit_to_fragment[unit_id] = block["FragmentPath"]
    return units, process_to_unit, unit_to_fragment, evidence


def parse_timers(grouped: dict[tuple[str, str], list[dict]]) -> tuple[dict[str, dict], list[tuple[str, str]], list[dict]]:
    timers: dict[str, dict] = {}
    unit_links: list[tuple[str, str]] = []
    evidence: list[dict] = []
    for item in grouped.get(("services", "systemctl"), []):
        argv = item["argv"]
        argv_text = " ".join(argv)
        if "list-timers" not in argv_text:
            continue
        evidence.append(item)
        for line in item["stdout"].splitlines():
            parts = re.split(r"\s{2,}|\t+", line.strip())
            if len(parts) < 6:
                continue
            timer_id = parts[-2]
            unit_id = parts[-1]
            if not timer_id.endswith(".timer"):
                continue
            timers.setdefault(
                timer_id,
                {
                    "kind": "timer",
                    "natural_key": f"timer:{timer_id}",
                    "title": timer_id,
                    "attrs": {
                        "unit": unit_id,
                    },
                },
            )
            if unit_id.endswith(".service"):
                unit_links.append((unit_id, timer_id))
    for item in grouped.get(("services", "systemctl"), []):
        argv = item["argv"]
        argv_text = " ".join(argv)
        if "show" not in argv or (
            "show --type=timer" not in argv_text and not any(token.endswith(".timer") for token in argv)
        ):
            continue
        evidence.append(item)
        for block in parse_service_blocks(item["stdout"]):
            timer_id = block.get("Id")
            if not timer_id:
                continue
            natural_key = f"timer:{timer_id}"
            timers[timer_id] = {
                "kind": "timer",
                "natural_key": natural_key,
                "title": timer_id,
                "attrs": {
                    "unit": block.get("Unit"),
                    "next_elapse": block.get("NextElapseUSecRealtime"),
                    "last_trigger": block.get("LastTriggerUSec"),
                    "fragment_path": block.get("FragmentPath"),
                    "description": block.get("Description"),
                },
            }
            if block.get("Unit"):
                unit_links.append((block["Unit"], timer_id))
    return timers, unit_links, evidence


def parse_repo(grouped: dict[tuple[str, str], list[dict]]) -> tuple[dict | None, dict[str, dict], dict[str, set[str]], list[dict]]:
    repo_entity = None
    repo_files: dict[str, dict] = {}
    hints: dict[str, set[str]] = {}
    evidence: list[dict] = []
    repo_target = None
    for item in grouped.get(("repo_inventory", "rg"), []):
        evidence.append(item)
        if repo_target is None:
            repo_target = item["target"]
    if repo_target:
        repo_entity = {
            "kind": "repo",
            "natural_key": f"repo:{repo_target}",
            "title": Path(repo_target).name or repo_target,
            "attrs": {"path": repo_target},
        }
    for item in grouped.get(("repo_inventory", "rg"), []):
        argv = " ".join(item["argv"])
        if "--files" in argv:
            continue
        for line in item["stdout"].splitlines():
            if ":" not in line:
                continue
            file_path, _, match_text = line.partition(":")
            natural_key = f"repo_file:{file_path}"
            repo_files.setdefault(
                file_path,
                {
                    "kind": "repo_file",
                    "natural_key": natural_key,
                    "title": file_path,
                    "attrs": {"path": file_path},
                },
            )
            hints.setdefault(file_path, set()).add(match_text)
    return repo_entity, repo_files, hints, evidence


def parse_journal_findings(grouped: dict[tuple[str, str], list[dict]]) -> tuple[dict[str, dict], list[dict]]:
    findings: dict[str, dict] = {}
    evidence: list[dict] = []
    for item in grouped.get(("journals", "journalctl"), []):
        evidence.append(item)
        per_unit: dict[str, list[str]] = {}
        for line in item["stdout"].splitlines():
            if not JOURNAL_SIGNAL_RE.search(line):
                continue
            match = SERVICE_UNIT_RE.search(line)
            if not match:
                continue
            unit_id = match.group(1)
            per_unit.setdefault(unit_id, []).append(line.strip())
        for unit_id, samples in per_unit.items():
            natural_key = f"journal_finding:{unit_id}"
            findings[unit_id] = {
                "kind": "journal_finding",
                "natural_key": natural_key,
                "title": f"journal finding for {unit_id}",
                "attrs": {
                    "source_unit": unit_id,
                    "severity": "warning",
                    "sample_count": len(samples),
                    "samples": samples[:3],
                },
            }
    return findings, evidence


def build_graph(conn: sqlite3.Connection, run_id: str) -> dict:
    captures = load_captures(conn, run_id)
    if not captures:
        raise SystemExit(f"no discovery captures found for run_id={run_id}")
    grouped = capture_map(captures)
    host_name, host_sources = parse_hostname(grouped)
    host = {
        "kind": "host",
        "natural_key": f"host:{host_name}",
        "title": host_name,
        "attrs": {"hostname": host_name},
    }

    processes, process_sources = parse_processes(grouped)
    process_cgroup_units, cgroup_sources = parse_process_cgroups(grouped)
    listeners, listener_binds, listener_sources = parse_listeners(grouped)
    units, process_to_unit, unit_to_fragment, unit_sources = parse_units(grouped)
    for pid, unit_id in process_cgroup_units.items():
        process_to_unit.setdefault(pid, unit_id)
    timers, timer_links, timer_sources = parse_timers(grouped)
    repo, repo_files, repo_hints, repo_sources = parse_repo(grouped)
    findings, journal_sources = parse_journal_findings(grouped)

    entities = [host]
    entities.extend(processes.values())
    entities.extend(listeners)
    entities.extend(units.values())
    entities.extend(timers.values())
    if repo:
        entities.append(repo)
        entities.extend(repo_files.values())
    entities.extend(findings.values())

    relations = []
    evidence = []

    def add_entity_evidence(source_items: list[dict], entity: dict, note: str) -> None:
        for item in source_items:
            evidence.append(
                {
                    "capture_id": item["capture_id"],
                    "entity": {
                        "kind": entity["kind"],
                        "natural_key": entity["natural_key"],
                    },
                    "note": note,
                }
            )

    add_entity_evidence(host_sources, host, "Derived from host identity output.")
    for entity in processes.values():
        add_entity_evidence(process_sources, entity, "Derived from ps output.")
        add_entity_evidence(cgroup_sources, entity, "Observed in process cgroup output.")
        relations.append(
            {
                "from": {"kind": "process", "natural_key": entity["natural_key"]},
                "relation": "runs_on",
                "to": {"kind": "host", "natural_key": host["natural_key"]},
                "attrs": {},
            }
        )
    for entity in listeners:
        add_entity_evidence(listener_sources, entity, "Derived from ss listener output.")
    for entity in units.values():
        add_entity_evidence(unit_sources, entity, "Derived from systemctl show service output.")
        relations.append(
            {
                "from": {"kind": "systemd_unit", "natural_key": entity["natural_key"]},
                "relation": "runs_on",
                "to": {"kind": "host", "natural_key": host["natural_key"]},
                "attrs": {},
            }
        )
    for entity in timers.values():
        add_entity_evidence(timer_sources, entity, "Derived from systemctl show timer output.")
        relations.append(
            {
                "from": {"kind": "timer", "natural_key": entity["natural_key"]},
                "relation": "runs_on",
                "to": {"kind": "host", "natural_key": host["natural_key"]},
                "attrs": {},
            }
        )
    if repo:
        add_entity_evidence(repo_sources, repo, "Derived from repo inventory output.")
        for entity in repo_files.values():
            add_entity_evidence(repo_sources, entity, "Derived from repo inventory matches.")
            relations.append(
                {
                    "from": {"kind": "repo_file", "natural_key": entity["natural_key"]},
                    "relation": "contains",
                    "to": {"kind": "repo", "natural_key": repo["natural_key"]},
                    "attrs": {},
                }
            )
    for entity in findings.values():
        add_entity_evidence(journal_sources, entity, "Derived from journal warning output.")

    for listener, pid in listener_binds:
        if pid not in processes:
            continue
        relations.append(
            {
                "from": {"kind": "listener", "natural_key": listener["natural_key"]},
                "relation": "managed_by",
                "to": {"kind": "process", "natural_key": processes[pid]["natural_key"]},
                "attrs": {},
            }
        )

    for pid, unit_id in process_to_unit.items():
        if pid not in processes or unit_id not in units:
            continue
        relations.append(
            {
                "from": {"kind": "process", "natural_key": processes[pid]["natural_key"]},
                "relation": "managed_by",
                "to": {"kind": "systemd_unit", "natural_key": units[unit_id]["natural_key"]},
                "attrs": {},
            }
        )

    for unit_id, fragment_path in unit_to_fragment.items():
        basename = Path(fragment_path).name
        for file_path, entity in repo_files.items():
            if Path(file_path).name == basename or unit_id in "".join(sorted(repo_hints.get(file_path, set()))):
                relations.append(
                    {
                        "from": {"kind": "systemd_unit", "natural_key": units[unit_id]["natural_key"]},
                        "relation": "defined_in",
                        "to": {"kind": "repo_file", "natural_key": entity["natural_key"]},
                        "attrs": {"fragment_path": fragment_path},
                    }
                )

    for unit_id, timer_id in timer_links:
        if timer_id not in timers:
            continue
        if unit_id not in units:
            units[unit_id] = {
                "kind": "systemd_unit",
                "natural_key": f"systemd_unit:{unit_id}",
                "title": unit_id,
                "attrs": {"inferred_from_timer": True},
            }
            entities.append(units[unit_id])
        relations.append(
            {
                "from": {"kind": "systemd_unit", "natural_key": units[unit_id]["natural_key"]},
                "relation": "scheduled_by",
                "to": {"kind": "timer", "natural_key": timers[timer_id]["natural_key"]},
                "attrs": {},
            }
        )

    for unit_id, finding in findings.items():
        if unit_id in units:
            relations.append(
                {
                    "from": {"kind": "journal_finding", "natural_key": finding["natural_key"]},
                    "relation": "about",
                    "to": {"kind": "systemd_unit", "natural_key": units[unit_id]["natural_key"]},
                    "attrs": {},
                }
            )

    return {
        "run_id": run_id,
        "skill_key": "discovery_graph",
        "status": "normalized",
        "full_sweep": True,
        "note": "Deterministic minimum normalization from stored discovery captures.",
        "entities": entities,
        "relations": relations,
        "evidence": evidence,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Normalize minimum discovery graph facts from stored captures.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()

    conn = open_db(args.db)
    payload = build_graph(conn, args.run_id)
    print(json.dumps(payload, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
