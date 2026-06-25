#!/usr/bin/env python3
"""Collect CTOX idle/status/SQLite performance diagnostics.

The probe is read-only by default. It samples process CPU without invoking
`ctox status`, then samples status latency separately, then inspects local
SQLite files in read-only mode.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
from pathlib import Path
import re
import shlex
import sqlite3
import statistics
import subprocess
import sys
import time
from typing import Any


SCHEMA = "ctox.perf_probe.v1"
DEFAULT_PROCESS_NAME = "ctox-real"
DEFAULT_DATABASES = (
    ("core", "runtime/ctox.sqlite3"),
    ("secrets", "runtime/ctox-secrets.sqlite3"),
    ("business_os", "runtime/business-os.sqlite3"),
    ("business_os_rxdb", "runtime/business-os-rxdb.sqlite3"),
)
RXDB_TABLE_RE = re.compile(r"^ctox_business_os__(?P<collection>.+)__v(?P<version>[0-9]+)$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Collect read-only CTOX performance diagnostics as JSON.",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path.cwd(),
        help="CTOX checkout/runtime root. Defaults to the current working directory.",
    )
    parser.add_argument("--pid", type=int, help="PID to sample instead of using pgrep.")
    parser.add_argument(
        "--process-name",
        default=DEFAULT_PROCESS_NAME,
        help=f"Process name used when --pid is omitted. Default: {DEFAULT_PROCESS_NAME}.",
    )
    parser.add_argument("--skip-cpu", action="store_true", help="Skip process CPU sampling.")
    parser.add_argument("--cpu-samples", type=int, default=10, help="CPU sample count.")
    parser.add_argument(
        "--cpu-interval",
        type=float,
        default=1.0,
        help="Seconds between CPU samples.",
    )
    parser.add_argument("--skip-status", action="store_true", help="Skip status latency sampling.")
    parser.add_argument(
        "--status-command",
        default="ctox status --json",
        help="Status command to time. Parsed with shell-like quoting, not run through a shell.",
    )
    parser.add_argument("--status-samples", type=int, default=5, help="Status sample count.")
    parser.add_argument(
        "--status-timeout",
        type=float,
        default=10.0,
        help="Per-status-command timeout in seconds.",
    )
    parser.add_argument(
        "--db",
        action="append",
        default=[],
        metavar="NAME=PATH",
        help="Additional SQLite DB to inspect. Relative paths are resolved under --root.",
    )
    parser.add_argument("--skip-db", action="store_true", help="Skip SQLite DB diagnostics.")
    parser.add_argument(
        "--max-tables",
        type=int,
        default=40,
        help="Maximum per-DB table summaries to include.",
    )
    parser.add_argument(
        "--max-dbstat-rows",
        type=int,
        default=30,
        help="Maximum dbstat rows to include per database.",
    )
    parser.add_argument(
        "--max-chunk-rows",
        type=int,
        default=200_000,
        help="Maximum desktop_file_chunks rows to parse for generation diagnostics.",
    )
    parser.add_argument(
        "--retain-chunk-generations",
        type=int,
        default=2,
        help="Live desktop_file_chunks generations per file treated as retained.",
    )
    parser.add_argument("--pretty", action="store_true", help="Pretty-print JSON.")
    args = parser.parse_args()

    if args.cpu_samples < 1:
        parser.error("--cpu-samples must be >= 1")
    if args.cpu_interval < 0:
        parser.error("--cpu-interval must be >= 0")
    if args.status_samples < 1:
        parser.error("--status-samples must be >= 1")
    if args.status_timeout <= 0:
        parser.error("--status-timeout must be > 0")
    if args.max_tables < 1:
        parser.error("--max-tables must be >= 1")
    if args.max_dbstat_rows < 1:
        parser.error("--max-dbstat-rows must be >= 1")
    if args.max_chunk_rows < 1:
        parser.error("--max-chunk-rows must be >= 1")
    if args.retain_chunk_generations < 0:
        parser.error("--retain-chunk-generations must be >= 0")
    return args


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def percentile(values: list[float], pct: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    rank = (len(ordered) - 1) * pct
    lower = int(rank)
    upper = min(lower + 1, len(ordered) - 1)
    if lower == upper:
        return ordered[lower]
    weight = rank - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def int_or_none(value: str) -> int | None:
    value = value.strip()
    if not value or value == "-":
        return None
    try:
        return int(value)
    except ValueError:
        return None


def float_or_none(value: str) -> float | None:
    value = value.strip().replace(",", ".")
    if not value or value == "-":
        return None
    try:
        return float(value)
    except ValueError:
        return None


def size_or_zero(path: Path) -> int:
    try:
        return path.stat().st_size
    except OSError:
        return 0


def resolve_pid(pid: int | None, process_name: str) -> dict[str, Any]:
    if pid is not None:
        return {
            "pid": pid,
            "process_name": process_name,
            "source": "argument",
            "candidates": [pid],
        }

    try:
        result = subprocess.run(
            ["pgrep", "-x", process_name],
            text=True,
            capture_output=True,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as err:
        return {
            "pid": None,
            "process_name": process_name,
            "source": "pgrep",
            "candidates": [],
            "error": str(err),
        }

    candidates: list[int] = []
    for raw in result.stdout.splitlines():
        raw = raw.strip()
        if not raw:
            continue
        try:
            candidates.append(int(raw))
        except ValueError:
            continue

    return {
        "pid": max(candidates) if candidates else None,
        "process_name": process_name,
        "source": "pgrep -x",
        "candidates": candidates,
        "returncode": result.returncode,
        "stderr": result.stderr.strip() or None,
    }


def read_process_sample(pid: int) -> dict[str, Any]:
    result = subprocess.run(
        [
            "ps",
            "-o",
            "pid=,pcpu=,time=,wq=,wqb=,wqr=,nvcsw=,nivcsw=",
            "-p",
            str(pid),
        ],
        text=True,
        capture_output=True,
        timeout=5,
        check=False,
    )
    sample: dict[str, Any] = {
        "at": utc_now(),
        "returncode": result.returncode,
    }
    line = result.stdout.strip().splitlines()
    if result.returncode != 0 or not line:
        sample["error"] = (result.stderr.strip() or "process sample failed")[:400]
        return sample

    parts = line[-1].split()
    if len(parts) < 8:
        sample["error"] = f"unexpected ps output: {line[-1]}"
        return sample

    sample.update(
        {
            "pid": int_or_none(parts[0]),
            "cpu_percent": float_or_none(parts[1]),
            "cpu_time": parts[2],
            "workqueue_threads": int_or_none(parts[3]),
            "workqueue_blocked": int_or_none(parts[4]),
            "workqueue_running": int_or_none(parts[5]),
            "voluntary_context_switches": int_or_none(parts[6]),
            "involuntary_context_switches": int_or_none(parts[7]),
        }
    )
    return sample


def sample_process(pid: int | None, process_name: str, count: int, interval: float) -> dict[str, Any]:
    resolved = resolve_pid(pid, process_name)
    selected_pid = resolved.get("pid")
    if selected_pid is None:
        return {
            "ok": False,
            "pid_resolution": resolved,
            "samples": [],
            "summary": None,
            "note": "No matching process found; pass --pid to sample a specific process.",
        }

    samples = []
    for index in range(count):
        samples.append(read_process_sample(int(selected_pid)))
        if index + 1 < count and interval > 0:
            time.sleep(interval)

    cpu_values = [
        sample["cpu_percent"]
        for sample in samples
        if isinstance(sample.get("cpu_percent"), (int, float))
    ]
    first = samples[0] if samples else {}
    last = samples[-1] if samples else {}
    summary = {
        "sample_count": len(samples),
        "interval_seconds": interval,
        "cpu_percent_avg": statistics.fmean(cpu_values) if cpu_values else None,
        "cpu_percent_min": min(cpu_values) if cpu_values else None,
        "cpu_percent_max": max(cpu_values) if cpu_values else None,
        "cpu_percent_p95": percentile(cpu_values, 0.95),
        "voluntary_context_switch_delta": counter_delta(
            first.get("voluntary_context_switches"),
            last.get("voluntary_context_switches"),
        ),
        "involuntary_context_switch_delta": counter_delta(
            first.get("involuntary_context_switches"),
            last.get("involuntary_context_switches"),
        ),
    }
    return {
        "ok": bool(cpu_values),
        "pid_resolution": resolved,
        "samples": samples,
        "summary": summary,
        "note": "CPU sampling does not invoke ctox status.",
    }


def counter_delta(first: Any, last: Any) -> int | None:
    if isinstance(first, int) and isinstance(last, int):
        return last - first
    return None


def sample_status(command_text: str, count: int, timeout: float, cwd: Path) -> dict[str, Any]:
    try:
        command = shlex.split(command_text)
    except ValueError as err:
        return {"ok": False, "command": command_text, "samples": [], "error": str(err)}
    if not command:
        return {"ok": False, "command": command_text, "samples": [], "error": "empty command"}

    samples = []
    for _ in range(count):
        started = time.perf_counter()
        try:
            result = subprocess.run(
                command,
                cwd=cwd,
                text=True,
                capture_output=True,
                timeout=timeout,
                check=False,
            )
            elapsed_ms = (time.perf_counter() - started) * 1000.0
            parsed_json = None
            if result.returncode == 0 and result.stdout.strip():
                try:
                    parsed_json = json.loads(result.stdout)
                except json.JSONDecodeError:
                    parsed_json = None
            samples.append(
                {
                    "at": utc_now(),
                    "latency_ms": elapsed_ms,
                    "returncode": result.returncode,
                    "stdout_bytes": len(result.stdout.encode("utf-8", errors="replace")),
                    "stderr_tail": result.stderr[-1000:] or None,
                    "json_ok": parsed_json is not None,
                    "status_ok": parsed_json.get("ok") if isinstance(parsed_json, dict) else None,
                }
            )
        except subprocess.TimeoutExpired as err:
            samples.append(
                {
                    "at": utc_now(),
                    "latency_ms": (time.perf_counter() - started) * 1000.0,
                    "returncode": None,
                    "timeout": True,
                    "error": str(err),
                }
            )
        except OSError as err:
            samples.append(
                {
                    "at": utc_now(),
                    "latency_ms": (time.perf_counter() - started) * 1000.0,
                    "returncode": None,
                    "error": str(err),
                }
            )

    latencies = [
        sample["latency_ms"]
        for sample in samples
        if isinstance(sample.get("latency_ms"), (int, float)) and not sample.get("timeout")
    ]
    return {
        "ok": bool(latencies) and all(sample.get("returncode") == 0 for sample in samples),
        "command": command,
        "samples": samples,
        "summary": {
            "sample_count": len(samples),
            "latency_ms_avg": statistics.fmean(latencies) if latencies else None,
            "latency_ms_min": min(latencies) if latencies else None,
            "latency_ms_max": max(latencies) if latencies else None,
            "latency_ms_p95": percentile(latencies, 0.95),
        },
        "note": "Status latency is sampled after process CPU sampling.",
    }


def database_specs(root: Path, extras: list[str]) -> list[tuple[str, Path]]:
    specs = [(name, root / relative) for name, relative in DEFAULT_DATABASES]
    for raw in extras:
        if "=" in raw:
            name, path_text = raw.split("=", 1)
            name = name.strip() or "custom"
        else:
            name = "custom"
            path_text = raw
        path = Path(path_text).expanduser()
        if not path.is_absolute():
            path = root / path
        specs.append((name, path))
    return specs


def connect_read_only(path: Path) -> sqlite3.Connection:
    uri = path.resolve().as_uri() + "?mode=ro"
    conn = sqlite3.connect(uri, uri=True, timeout=2.0)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA query_only = ON")
    return conn


def inspect_databases(
    root: Path,
    extras: list[str],
    max_tables: int,
    max_dbstat_rows: int,
    max_chunk_rows: int,
    retain_chunk_generations: int,
) -> list[dict[str, Any]]:
    return [
        inspect_database(
            name,
            path,
            max_tables=max_tables,
            max_dbstat_rows=max_dbstat_rows,
            max_chunk_rows=max_chunk_rows,
            retain_chunk_generations=retain_chunk_generations,
        )
        for name, path in database_specs(root, extras)
    ]


def inspect_database(
    name: str,
    path: Path,
    *,
    max_tables: int,
    max_dbstat_rows: int,
    max_chunk_rows: int,
    retain_chunk_generations: int,
) -> dict[str, Any]:
    report: dict[str, Any] = {
        "name": name,
        "path": str(path),
        "exists": path.exists(),
        "file_bytes": size_or_zero(path),
        "wal_bytes": size_or_zero(Path(str(path) + "-wal")),
        "shm_bytes": size_or_zero(Path(str(path) + "-shm")),
    }
    if not path.exists():
        return report

    try:
        with connect_read_only(path) as conn:
            report["page_size"] = scalar(conn, "PRAGMA page_size")
            report["page_count"] = scalar(conn, "PRAGMA page_count")
            report["freelist_count"] = scalar(conn, "PRAGMA freelist_count")
            if isinstance(report.get("page_size"), int) and isinstance(report.get("page_count"), int):
                report["database_page_bytes"] = report["page_size"] * report["page_count"]
            if isinstance(report.get("page_size"), int) and isinstance(report.get("freelist_count"), int):
                report["freelist_bytes"] = report["page_size"] * report["freelist_count"]
            tables = table_names(conn)
            report["table_count"] = len(tables)
            report["tables"] = sorted(
                (inspect_table(conn, table) for table in tables),
                key=table_sort_key,
                reverse=True,
            )[:max_tables]
            report["rxdb_collections"] = top_rxdb_collections(report["tables"])
            report["dbstat"] = inspect_dbstat(conn, max_dbstat_rows)
            if "ctox_business_os__desktop_file_chunks__v0" in tables:
                report["desktop_file_chunks"] = inspect_desktop_file_chunks(
                    conn,
                    max_rows=max_chunk_rows,
                    retain_generations=retain_chunk_generations,
                )
    except Exception as err:  # The probe should report diagnostics, not crash.
        report["error"] = f"{type(err).__name__}: {err}"
    return report


def table_sort_key(item: dict[str, Any]) -> tuple[int, int]:
    data_bytes = item.get("data_bytes")
    row_count = item.get("row_count")
    return (
        data_bytes if isinstance(data_bytes, int) and data_bytes > 0 else 0,
        row_count if isinstance(row_count, int) else 0,
    )


def scalar(conn: sqlite3.Connection, sql: str) -> Any:
    row = conn.execute(sql).fetchone()
    if row is None:
        return None
    return row[0]


def quote_identifier(identifier: str) -> str:
    return '"' + identifier.replace('"', '""') + '"'


def table_names(conn: sqlite3.Connection) -> list[str]:
    return [
        str(row[0])
        for row in conn.execute(
            "SELECT name FROM sqlite_master "
            "WHERE type = 'table' AND name NOT LIKE 'sqlite_%' "
            "ORDER BY name"
        )
    ]


def table_columns(conn: sqlite3.Connection, table: str) -> set[str]:
    return {str(row["name"]) for row in conn.execute(f"PRAGMA table_info({quote_identifier(table)})")}


def inspect_table(conn: sqlite3.Connection, table: str) -> dict[str, Any]:
    columns = table_columns(conn, table)
    quoted = quote_identifier(table)
    report: dict[str, Any] = {
        "table": table,
        "columns": sorted(columns),
        "row_count": query_scalar(conn, f"SELECT COUNT(*) FROM {quoted}"),
    }
    rxdb_match = RXDB_TABLE_RE.match(table)
    if rxdb_match:
        report["rxdb_collection"] = rxdb_match.group("collection")
        report["rxdb_schema_version"] = int(rxdb_match.group("version"))
    if "deleted" in columns:
        report["tombstone_count"] = query_scalar(
            conn,
            f"SELECT SUM(CASE WHEN COALESCE(deleted, 0) != 0 THEN 1 ELSE 0 END) FROM {quoted}",
        ) or 0
    if "lastWriteTime" in columns:
        report["last_write_time_max"] = query_scalar(conn, f"SELECT MAX(lastWriteTime) FROM {quoted}")
    if "data" in columns:
        report["data_bytes"] = query_scalar(conn, f"SELECT SUM(LENGTH(data)) FROM {quoted}") or 0
    return report


def query_scalar(conn: sqlite3.Connection, sql: str, params: tuple[Any, ...] = ()) -> Any:
    try:
        row = conn.execute(sql, params).fetchone()
    except sqlite3.DatabaseError as err:
        return {"error": str(err)}
    if row is None:
        return None
    return row[0]


def top_rxdb_collections(tables: list[dict[str, Any]]) -> list[dict[str, Any]]:
    collections = []
    for table in tables:
        collection = table.get("rxdb_collection")
        if not collection:
            continue
        collections.append(
            {
                "collection": collection,
                "schema_version": table.get("rxdb_schema_version"),
                "table": table.get("table"),
                "row_count": table.get("row_count"),
                "tombstone_count": table.get("tombstone_count"),
                "data_bytes": table.get("data_bytes"),
                "last_write_time_max": table.get("last_write_time_max"),
            }
        )
    return collections


def inspect_dbstat(conn: sqlite3.Connection, limit: int) -> dict[str, Any]:
    try:
        rows = conn.execute(
            "SELECT name, SUM(pgsize) AS bytes, COUNT(*) AS pages "
            "FROM dbstat GROUP BY name ORDER BY bytes DESC LIMIT ?",
            (limit,),
        ).fetchall()
    except sqlite3.DatabaseError as err:
        return {"available": False, "error": str(err)}
    return {
        "available": True,
        "objects": [
            {"name": row["name"], "bytes": row["bytes"], "pages": row["pages"]}
            for row in rows
        ],
    }


def inspect_desktop_file_chunks(
    conn: sqlite3.Connection,
    *,
    max_rows: int,
    retain_generations: int,
) -> dict[str, Any]:
    table = "ctox_business_os__desktop_file_chunks__v0"
    columns = table_columns(conn, table)
    quoted = quote_identifier(table)
    row_count = query_scalar(conn, f"SELECT COUNT(*) FROM {quoted}")
    select_deleted = ", deleted" if "deleted" in columns else ""
    rows = conn.execute(f"SELECT data{select_deleted} FROM {quoted} LIMIT ?", (max_rows + 1,)).fetchall()
    truncated = len(rows) > max_rows
    if truncated:
        rows = rows[:max_rows]

    generations: dict[tuple[str, str], dict[str, Any]] = {}
    tombstone_rows = 0
    tombstone_bytes = 0
    malformed_rows = 0
    for row in rows:
        raw = row["data"]
        deleted_column = bool(row["deleted"]) if "deleted" in row.keys() else False
        try:
            doc = json.loads(raw)
        except (TypeError, json.JSONDecodeError):
            malformed_rows += 1
            continue
        deleted_doc = bool(doc.get("_deleted")) or bool(doc.get("deleted") is True)
        size_bytes = document_chunk_size(doc)
        if deleted_column or deleted_doc:
            tombstone_rows += 1
            tombstone_bytes += size_bytes
            continue
        file_id = string_value(doc.get("file_id"))
        generation_id = string_value(doc.get("generation_id"))
        if not file_id or not generation_id:
            malformed_rows += 1
            continue
        key = (file_id, generation_id)
        group = generations.setdefault(
            key,
            {
                "file_id": file_id,
                "generation_id": generation_id,
                "chunk_count": 0,
                "bytes": 0,
                "max_idx": None,
                "declared_total": None,
                "created_at_ms": None,
                "updated_at_ms": None,
            },
        )
        group["chunk_count"] += 1
        group["bytes"] += size_bytes
        merge_max(group, "max_idx", int_value(doc.get("idx")))
        merge_max(group, "declared_total", int_value(doc.get("total")))
        merge_max(group, "created_at_ms", int_value(doc.get("created_at_ms")))
        merge_max(group, "updated_at_ms", int_value(doc.get("updated_at_ms")))

    by_file: dict[str, list[dict[str, Any]]] = {}
    for group in generations.values():
        by_file.setdefault(group["file_id"], []).append(group)

    retained = []
    stale = []
    for file_generations in by_file.values():
        file_generations.sort(key=generation_sort_key, reverse=True)
        retained.extend(file_generations[:retain_generations])
        stale.extend(file_generations[retain_generations:])

    stale.sort(key=lambda item: item.get("bytes") or 0, reverse=True)
    retained.sort(key=lambda item: item.get("bytes") or 0, reverse=True)

    return {
        "row_count": row_count,
        "sampled_rows": len(rows),
        "truncated": truncated,
        "max_rows": max_rows,
        "file_count": len(by_file),
        "live_generation_count": len(generations),
        "live_bytes_sampled": sum(group.get("bytes") or 0 for group in generations.values()),
        "retained_generation_count": len(retained),
        "retained_bytes_sampled": sum(group.get("bytes") or 0 for group in retained),
        "stale_generation_count": len(stale),
        "stale_bytes_sampled": sum(group.get("bytes") or 0 for group in stale),
        "tombstone_rows_sampled": tombstone_rows,
        "tombstone_bytes_sampled": tombstone_bytes,
        "malformed_rows_sampled": malformed_rows,
        "retain_generations_per_file": retain_generations,
        "top_stale_generations": stale[:20],
        "top_retained_generations": retained[:20],
    }


def string_value(value: Any) -> str | None:
    if isinstance(value, str) and value:
        return value
    return None


def int_value(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        return int(value)
    if isinstance(value, str):
        try:
            return int(float(value))
        except ValueError:
            return None
    return None


def merge_max(group: dict[str, Any], key: str, value: int | None) -> None:
    if value is None:
        return
    existing = group.get(key)
    if existing is None or value > existing:
        group[key] = value


def generation_sort_key(group: dict[str, Any]) -> tuple[int, str]:
    timestamp = group.get("created_at_ms") or group.get("updated_at_ms") or 0
    return (int(timestamp), str(group.get("generation_id") or ""))


def document_chunk_size(doc: dict[str, Any]) -> int:
    explicit = int_value(doc.get("size_bytes"))
    if explicit is not None and explicit >= 0:
        return explicit
    payload = doc.get("data")
    if isinstance(payload, str):
        return (len(payload) * 3) // 4
    return 0


def main() -> int:
    args = parse_args()
    root = args.root.expanduser().resolve()
    report: dict[str, Any] = {
        "schema": SCHEMA,
        "generated_at": utc_now(),
        "root": str(root),
        "host": {
            "platform": sys.platform,
            "cwd": os.getcwd(),
        },
    }

    if args.skip_cpu:
        report["process"] = {"skipped": True}
    else:
        report["process"] = sample_process(
            args.pid,
            args.process_name,
            args.cpu_samples,
            args.cpu_interval,
        )

    if args.skip_status:
        report["status_latency"] = {"skipped": True}
    else:
        report["status_latency"] = sample_status(
            args.status_command,
            args.status_samples,
            args.status_timeout,
            root,
        )

    if args.skip_db:
        report["databases"] = {"skipped": True}
    else:
        report["databases"] = inspect_databases(
            root,
            args.db,
            args.max_tables,
            args.max_dbstat_rows,
            args.max_chunk_rows,
            args.retain_chunk_generations,
        )

    json.dump(report, sys.stdout, indent=2 if args.pretty else None, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
