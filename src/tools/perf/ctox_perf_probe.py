#!/usr/bin/env python3
"""Collect CTOX idle/status/SQLite performance diagnostics.

The probe is read-only by default. It samples process CPU without invoking
`ctox status`, then samples status latency separately, then inspects local
SQLite files in read-only mode.
"""

from __future__ import annotations

import argparse
import datetime as dt
import fnmatch
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
EXPECTED_NATIVE_PEER_STATUS_VERSION = "ctox-native-rxdb-peer-status-v1"
EXPECTED_NATIVE_PEER_PERFORMANCE_SCHEMA = "ctox.native_peer.performance.v1"
EXPECTED_NATIVE_PEER_SQLITE_COUNTER_SCHEMA = "ctox.rxdb.sqlite.runtime_counters.v1"
EXPECTED_NATIVE_PEER_SUBJECT_COUNTER_SCHEMA = "ctox.rxdb.subjects.runtime_counters.v1"
DEFAULT_ASSERT_CPU_AVG = 2.0
DEFAULT_ASSERT_CPU_P95 = 5.0
DEFAULT_ASSERT_STATUS_P95_MS = 100.0
DEFAULT_ASSERT_DB_GROWTH_BYTES = 0
DEFAULT_ASSERT_DB_FILE_GROWTH_BYTES = 0
DEFAULT_ASSERT_HEARTBEAT_MAX_AGE_MS = 30_000
DEFAULT_ASSERT_DB_METRIC_DELTAS: tuple[tuple[str, float], ...] = (
    ("*.page_count", 0.0),
    ("*.database_page_bytes", 0.0),
    ("*.freelist_count", 0.0),
    ("*.freelist_bytes", 0.0),
    ("*.dbstat.*.bytes", 0.0),
    ("*.rxdb_collections.*.row_count", 0.0),
    ("*.rxdb_collections.*.data_bytes", 0.0),
    ("*.rxdb_collections.*.tombstone_count", 0.0),
    ("*.desktop_file_chunks.row_count", 0.0),
    ("*.desktop_file_chunks.live_generation_count", 0.0),
    ("*.desktop_file_chunks.live_bytes_sampled", 0.0),
    ("*.desktop_file_chunks.stale_generation_count", 0.0),
    ("*.desktop_file_chunks.stale_bytes_sampled", 0.0),
    ("*.desktop_file_chunks.tombstone_rows_sampled", 0.0),
    ("*.desktop_file_chunks.tombstone_bytes_sampled", 0.0),
)
DEFAULT_ASSERT_HEARTBEAT_DELTAS: tuple[tuple[str, float], ...] = (
    ("rxdb_sqlite.bulk_write_calls", 0.0),
    ("rxdb_sqlite.bulk_write_rows", 0.0),
    ("rxdb_sqlite.changed_documents_since_calls", 0.0),
    ("rxdb_sqlite.changed_documents_since_results", 0.0),
    ("rxdb_sqlite.external_poll_data_version_reads", 0.0),
    ("rxdb_sqlite.external_poll_changed_table_reads", 0.0),
    ("rxdb_sqlite.external_poll_connection_open_failures", 0.0),
    ("rxdb_sqlite.external_poll_wakeups", 0.0),
    ("rxdb_sqlite.external_poll_active_wakeups", 0.0),
    ("rxdb_sqlite.external_poll_standby_wakeups", 0.0),
    ("rxdb_sqlite.external_poll_standby_entries", 0.0),
    ("rxdb_sqlite.external_poll_active_resets", 0.0),
    ("rxdb_sqlite.external_poll_data_version_changes", 0.0),
    ("rxdb_sqlite.external_poll_data_version_read_failures", 0.0),
    ("rxdb_sqlite.external_poll_changed_table_read_failures", 0.0),
    ("rxdb_sqlite.external_poll_changed_table_notifications", 0.0),
    ("rxdb_sqlite.external_poll_local_hook_suppressed_notifications", 0.0),
    ("rxdb_sqlite.external_poll_notifications_by_table.*", 0.0),
    ("rxdb_sqlite.external_poll_local_hook_suppressions_by_table.*", 0.0),
    ("rxdb_sqlite.external_poll_drain_calls", 0.0),
    ("rxdb_sqlite.external_poll_drain_batches", 0.0),
    ("rxdb_sqlite.external_poll_drain_empty_batches", 0.0),
    ("rxdb_sqlite.external_poll_drain_rows_visited", 0.0),
    ("rxdb_sqlite.external_poll_drain_rows_decoded", 0.0),
    ("rxdb_sqlite.external_poll_drain_rows_max", 0.0),
    ("rxdb_sqlite.external_poll_drain_batches_max", 0.0),
    ("rxdb_sqlite.external_poll_drain_budget_exhaustions", 0.0),
    ("rxdb_sqlite.external_poll_drain_rows_by_table.*", 0.0),
    ("rxdb_sqlite.external_poll_drain_batches_by_table.*", 0.0),
    ("rxdb_sqlite.external_poll_drain_budget_exhaustions_by_table.*", 0.0),
    ("rxdb_sqlite.count_calls", 0.0),
    ("rxdb_sqlite.count_fallback_query_calls", 0.0),
    ("rxdb_sqlite.find_documents_by_id_calls", 0.0),
    ("rxdb_sqlite.query_calls", 0.0),
    ("rxdb_sqlite.query_fallback_calls", 0.0),
    ("rxdb_sqlite.query_fallback_rows_visited", 0.0),
    ("rxdb_sqlite.query_fallback_rows_decoded", 0.0),
    ("rxdb_sqlite.query_fallback_indexed_candidate_calls", 0.0),
    ("rxdb_sqlite.query_fallback_too_broad_calls", 0.0),
    ("rxdb_sqlite.query_fallback_by_collection.*", 0.0),
    ("rxdb_sqlite.query_fallback_by_operator.*", 0.0),
    ("rxdb_sqlite.query_fallback_by_collection_operator.*.*", 0.0),
    ("rxdb_sqlite.query_fallback_rows_visited_by_collection.*", 0.0),
    ("rxdb_sqlite.query_fallback_rows_decoded_by_collection.*", 0.0),
    ("rxdb_sqlite.query_fallback_rows_visited_by_operator.*", 0.0),
    ("rxdb_sqlite.query_fallback_rows_decoded_by_operator.*", 0.0),
    ("rxdb_sqlite.query_fallback_rows_visited_by_collection_operator.*.*", 0.0),
    ("rxdb_sqlite.query_fallback_rows_decoded_by_collection_operator.*.*", 0.0),
    ("rxdb_sqlite.query_stream_calls", 0.0),
    ("rxdb_sqlite.query_stream_unsupported_calls", 0.0),
    ("rxdb_sqlite.read_only_open_failures", 0.0),
    ("rxdb_sqlite.statements_executed", 0.0),
    ("rxdb_sqlite.statement_elapsed_ns_total", 0.0),
    ("rxdb_sqlite.statement_elapsed_ge_1ms", 0.0),
    ("rxdb_sqlite.write_transactions_started", 0.0),
    ("rxdb_sqlite.write_transactions_committed", 0.0),
    ("rxdb_sqlite.write_transactions_failed", 0.0),
    ("rxdb_sqlite.writer_lock_acquire_calls", 0.0),
    ("rxdb_sqlite.writer_lock_wait_ns_total", 0.0),
    ("rxdb_sqlite.writer_lock_wait_ge_1ms", 0.0),
    ("rxdb_sqlite.writer_lock_held_ns_total", 0.0),
    ("rxdb_sqlite.writer_lock_held_ge_1ms", 0.0),
    ("rxdb_sqlite.writer_fallbacks", 0.0),
    ("rxdb_subjects.lagged_items_total", 0.0),
    ("loops.*.active_ticks", 0.0),
    ("loops.*.error_ticks", 0.0),
    ("loops.*.rows", 0.0),
)
DEFAULT_ASSERT_SERVICE_STATUS_DELTAS: tuple[tuple[str, float], ...] = (
    ("channel_sync.*.activity_runs", 0.0),
    ("channel_sync.*.no_activity_runs", 0.0),
    ("channel_sync.*.error_runs", 0.0),
)
DEFAULT_ASSERT_SERVICE_PERFORMANCE_DELTAS: tuple[tuple[str, float], ...] = (
    ("status_requests.total_requests", 0.0),
    ("status_requests.ipc_status_requests", 0.0),
    ("status_requests.http_status_requests", 0.0),
)
DEFAULT_ASSERT_SYNC_RUN_DELTAS: tuple[tuple[str, float], ...] = (
    ("*.ticket_sync_runs.row_count", 0.0),
    ("*.communication_sync_runs.row_count", 0.0),
)
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
        "--skip-heartbeat",
        action="store_true",
        help="Skip direct native RxDB peer heartbeat snapshots around CPU sampling.",
    )
    parser.add_argument(
        "--skip-service-performance",
        action="store_true",
        help=(
            "Skip direct service-performance status-file snapshots around CPU sampling. "
            "Use only for scenarios that intentionally create status load."
        ),
    )
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
    parser.add_argument(
        "--assert-idle",
        action="store_true",
        help="Evaluate idle budgets, include assertion results, and exit non-zero on failure.",
    )
    parser.add_argument(
        "--max-cpu-avg",
        type=float,
        help=(
            "Maximum average process CPU percent. "
            f"Default with --assert-idle: {DEFAULT_ASSERT_CPU_AVG}."
        ),
    )
    parser.add_argument(
        "--max-cpu-p95",
        type=float,
        help=(
            "Maximum p95 process CPU percent. "
            f"Default with --assert-idle: {DEFAULT_ASSERT_CPU_P95}."
        ),
    )
    parser.add_argument(
        "--max-cpu-max",
        type=float,
        help="Optional maximum single sampled process CPU percent.",
    )
    parser.add_argument(
        "--max-status-p95-ms",
        type=float,
        help=(
            "Maximum ctox status p95 latency in ms. "
            f"Default with --assert-idle: {DEFAULT_ASSERT_STATUS_P95_MS}."
        ),
    )
    parser.add_argument(
        "--max-db-growth-bytes",
        type=int,
        help=(
            "Maximum total SQLite file growth during the CPU sample window. "
            f"Default with --assert-idle: {DEFAULT_ASSERT_DB_GROWTH_BYTES}."
        ),
    )
    parser.add_argument(
        "--max-db-file-growth-bytes",
        type=int,
        help=(
            "Maximum positive growth for any SQLite main/WAL/SHM/journal file "
            f"during the CPU sample window. Default with --assert-idle: "
            f"{DEFAULT_ASSERT_DB_FILE_GROWTH_BYTES}."
        ),
    )
    parser.add_argument(
        "--max-db-metric-delta",
        action="append",
        default=[],
        metavar="GLOB=VALUE",
        help=(
            "Maximum database diagnostic metric delta during the CPU sample window. "
            "May be repeated and supports shell-style globs, for example "
            "'*.rxdb_collections.*.tombstone_count=0'."
        ),
    )
    parser.add_argument(
        "--max-heartbeat-delta",
        action="append",
        default=[],
        metavar="GLOB=VALUE",
        help=(
            "Maximum native peer heartbeat performance delta for a flattened metric key. "
            "May be repeated and supports shell-style globs, for example "
            "'rxdb_sqlite.query_calls=0' or 'loops.*.rows=0'."
        ),
    )
    parser.add_argument(
        "--max-heartbeat-age-ms",
        type=int,
        help=(
            "Maximum age of runtime/business-os-rxdb-peer.status.json heartbeat "
            f"snapshots. Default with --assert-idle: {DEFAULT_ASSERT_HEARTBEAT_MAX_AGE_MS}."
        ),
    )
    parser.add_argument(
        "--max-service-status-delta",
        action="append",
        default=[],
        metavar="GLOB=VALUE",
        help=(
            "Maximum service status performance delta for a flattened metric key. "
            "May be repeated and supports shell-style globs, for example "
            "'channel_sync.*.no_activity_runs=0'."
        ),
    )
    parser.add_argument(
        "--max-service-performance-delta",
        action="append",
        default=[],
        metavar="GLOB=VALUE",
        help=(
            "Maximum service performance-file delta during the CPU sample window. "
            "May be repeated and supports shell-style globs, for example "
            "'status_requests.total_requests=0'."
        ),
    )
    parser.add_argument(
        "--max-sync-run-delta",
        action="append",
        default=[],
        metavar="GLOB=VALUE",
        help=(
            "Maximum sync-run table delta during the CPU sample window. "
            "May be repeated and supports shell-style globs, for example "
            "'*.ticket_sync_runs.row_count=0'."
        ),
    )
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
    for raw in args.max_heartbeat_delta:
        try:
            parse_metric_threshold(raw)
        except ValueError as err:
            parser.error(f"--max-heartbeat-delta {err}")
    if args.max_heartbeat_age_ms is not None and args.max_heartbeat_age_ms < 0:
        parser.error("--max-heartbeat-age-ms must be >= 0")
    for raw in args.max_db_metric_delta:
        try:
            parse_metric_threshold(raw)
        except ValueError as err:
            parser.error(f"--max-db-metric-delta {err}")
    for raw in args.max_service_status_delta:
        try:
            parse_metric_threshold(raw)
        except ValueError as err:
            parser.error(f"--max-service-status-delta {err}")
    for raw in args.max_service_performance_delta:
        try:
            parse_metric_threshold(raw)
        except ValueError as err:
            parser.error(f"--max-service-performance-delta {err}")
    for raw in args.max_sync_run_delta:
        try:
            parse_metric_threshold(raw)
        except ValueError as err:
            parser.error(f"--max-sync-run-delta {err}")
    return args


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def epoch_ms() -> int:
    return int(time.time() * 1000)


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


def parse_metric_threshold(raw: str) -> tuple[str, float]:
    if "=" not in raw:
        raise ValueError(f"must use GLOB=VALUE, got {raw!r}")
    pattern, value_text = raw.split("=", 1)
    pattern = pattern.strip()
    value_text = value_text.strip()
    if not pattern:
        raise ValueError(f"must include a non-empty metric glob, got {raw!r}")
    try:
        value = float(value_text)
    except ValueError as err:
        raise ValueError(f"must use a numeric VALUE, got {raw!r}") from err
    return pattern, value


def size_or_zero(path: Path) -> int:
    try:
        return path.stat().st_size
    except OSError:
        return 0


def database_file_snapshot(root: Path, extras: list[str]) -> dict[str, Any]:
    databases = []
    total_bytes = 0
    for name, path in database_specs(root, extras):
        files = {
            "main": size_or_zero(path),
            "wal": size_or_zero(Path(str(path) + "-wal")),
            "shm": size_or_zero(Path(str(path) + "-shm")),
            "journal": size_or_zero(Path(str(path) + "-journal")),
        }
        database_total = sum(files.values())
        total_bytes += database_total
        databases.append(
            {
                "name": name,
                "path": str(path),
                "exists": path.exists(),
                "files": files,
                "total_bytes": database_total,
            }
        )
    return {
        "at": utc_now(),
        "total_bytes": total_bytes,
        "databases": databases,
    }


def database_file_growth(
    before: dict[str, Any] | None,
    after: dict[str, Any] | None,
) -> dict[str, Any] | None:
    if not isinstance(before, dict) or not isinstance(after, dict):
        return None
    before_items = before.get("databases")
    after_items = after.get("databases")
    if not isinstance(before_items, list) or not isinstance(after_items, list):
        return None
    before_by_path = {
        item.get("path"): item
        for item in before_items
        if isinstance(item, dict) and isinstance(item.get("path"), str)
    }
    databases = []
    for after_item in after_items:
        if not isinstance(after_item, dict) or not isinstance(after_item.get("path"), str):
            continue
        before_item = before_by_path.get(after_item["path"])
        before_total = before_item.get("total_bytes") if isinstance(before_item, dict) else None
        after_total = after_item.get("total_bytes")
        if isinstance(before_total, int) and isinstance(after_total, int):
            growth = after_total - before_total
        else:
            growth = None
        before_files = before_item.get("files") if isinstance(before_item, dict) else None
        after_files = after_item.get("files")
        file_growth = {}
        if isinstance(before_files, dict) and isinstance(after_files, dict):
            for component in sorted(set(before_files) | set(after_files)):
                before_size = before_files.get(component)
                after_size = after_files.get(component)
                if isinstance(before_size, int) and isinstance(after_size, int):
                    file_growth[component] = after_size - before_size
        positive_file_growth = {
            component: delta for component, delta in file_growth.items() if delta > 0
        }
        databases.append(
            {
                "name": after_item.get("name"),
                "path": after_item.get("path"),
                "before_total_bytes": before_total,
                "after_total_bytes": after_total,
                "growth_bytes": growth,
                "file_growth_bytes": file_growth,
                "positive_file_growth_bytes": positive_file_growth,
                "positive_file_growth_total_bytes": sum(positive_file_growth.values()),
            }
        )
    total_before = before.get("total_bytes")
    total_after = after.get("total_bytes")
    total_growth = (
        total_after - total_before
        if isinstance(total_before, int) and isinstance(total_after, int)
        else None
    )
    return {
        "before_at": before.get("at"),
        "after_at": after.get("at"),
        "total_before_bytes": total_before,
        "total_after_bytes": total_after,
        "total_growth_bytes": total_growth,
        "databases": databases,
    }


def sync_run_snapshot(root: Path, extras: list[str]) -> dict[str, Any]:
    databases = []
    for name, path in database_specs(root, extras):
        database_report: dict[str, Any] = {
            "name": name,
            "path": str(path),
            "exists": path.exists(),
            "tables": {},
        }
        if path.exists():
            try:
                with connect_read_only(path) as conn:
                    tables = set(table_names(conn))
                    for table in ("ticket_sync_runs", "communication_sync_runs"):
                        if table in tables:
                            database_report["tables"][table] = inspect_sync_run_table(conn, table)
            except Exception as err:
                database_report["error"] = f"{type(err).__name__}: {err}"
        databases.append(database_report)
    return {
        "at": utc_now(),
        "databases": databases,
        "note": "Sync-run counts are read-only snapshots taken around process CPU sampling.",
    }


def inspect_sync_run_table(conn: sqlite3.Connection, table: str) -> dict[str, Any]:
    columns = table_columns(conn, table)
    quoted = quote_identifier(table)
    report: dict[str, Any] = {
        "row_count": query_scalar(conn, f"SELECT COUNT(*) FROM {quoted}") or 0,
    }
    if "status" in columns:
        report["ok_count"] = query_scalar(conn, f"SELECT COUNT(*) FROM {quoted} WHERE status = 'ok'") or 0
        report["failed_count"] = query_scalar(
            conn,
            f"SELECT COUNT(*) FROM {quoted} WHERE status != 'ok'",
        ) or 0
    elif "ok" in columns:
        report["ok_count"] = query_scalar(
            conn,
            f"SELECT SUM(CASE WHEN COALESCE(ok, 0) != 0 THEN 1 ELSE 0 END) FROM {quoted}",
        ) or 0
        report["failed_count"] = query_scalar(
            conn,
            f"SELECT SUM(CASE WHEN COALESCE(ok, 0) = 0 THEN 1 ELSE 0 END) FROM {quoted}",
        ) or 0
    for column in (
        "fetched_count",
        "stored_count",
        "stored_ticket_count",
        "stored_event_count",
    ):
        if column in columns:
            report[f"{column}_total"] = query_scalar(
                conn,
                f"SELECT COALESCE(SUM({quote_identifier(column)}), 0) FROM {quoted}",
            ) or 0
    for column in ("created_at", "finished_at", "started_at"):
        if column in columns:
            report[f"{column}_max"] = query_scalar(
                conn,
                f"SELECT MAX({quote_identifier(column)}) FROM {quoted}",
            )
    return report


def sync_run_snapshot_numbers(snapshot: dict[str, Any] | None) -> dict[str, float]:
    if not isinstance(snapshot, dict):
        return {}
    numbers: dict[str, float] = {}
    databases = snapshot.get("databases")
    if not isinstance(databases, list):
        return numbers
    for database in databases:
        if not isinstance(database, dict):
            continue
        name = database.get("name")
        tables = database.get("tables")
        if not isinstance(name, str) or not isinstance(tables, dict):
            continue
        for table_name, table_report in tables.items():
            if not isinstance(table_name, str) or not isinstance(table_report, dict):
                continue
            for key, value in flatten_numeric_values(table_report).items():
                numbers[f"{name}.{table_name}.{key}"] = value
    return numbers


def sync_run_snapshot_delta(
    before: dict[str, Any] | None,
    after: dict[str, Any] | None,
) -> dict[str, Any] | None:
    if not isinstance(before, dict) or not isinstance(after, dict):
        return None
    before_numbers = sync_run_snapshot_numbers(before)
    after_numbers = sync_run_snapshot_numbers(after)
    deltas = {}
    for key in sorted(set(before_numbers) & set(after_numbers)):
        delta = after_numbers[key] - before_numbers[key]
        if delta:
            deltas[key] = delta
    return {
        "before_at": before.get("at"),
        "after_at": after.get("at"),
        "numeric_deltas": deltas,
        "note": (
            "Deltas count sync-run rows and totals created during the CPU sample window "
            "without invoking ctox status."
        ),
    }


def read_json_file(path: Path) -> dict[str, Any]:
    report: dict[str, Any] = {
        "path": str(path),
        "exists": path.exists(),
        "read_at": utc_now(),
        "read_at_epoch_ms": epoch_ms(),
    }
    if not path.exists():
        return report
    try:
        report["payload"] = json.loads(path.read_text(encoding="utf-8"))
    except Exception as err:
        report["error"] = f"{type(err).__name__}: {err}"
    return report


def read_native_peer_heartbeat(root: Path) -> dict[str, Any]:
    return read_json_file(root / "runtime" / "business-os-rxdb-peer.status.json")


def read_service_performance_status(root: Path) -> dict[str, Any]:
    return read_json_file(root / "runtime" / "service-performance.status.json")


def flatten_numeric_values(value: Any, prefix: str = "") -> dict[str, float]:
    if isinstance(value, bool):
        return {}
    if isinstance(value, (int, float)):
        return {prefix: float(value)}
    if isinstance(value, dict):
        flattened: dict[str, float] = {}
        for key, child in value.items():
            child_prefix = f"{prefix}.{key}" if prefix else str(key)
            flattened.update(flatten_numeric_values(child, child_prefix))
        return flattened
    return {}


def native_peer_heartbeat_delta(
    before: dict[str, Any] | None,
    after: dict[str, Any] | None,
) -> dict[str, Any] | None:
    if not before or not after:
        return None
    before_payload = before.get("payload")
    after_payload = after.get("payload")
    if not isinstance(before_payload, dict) or not isinstance(after_payload, dict):
        return None
    before_perf = before_payload.get("performance")
    after_perf = after_payload.get("performance")
    if not isinstance(before_perf, dict) or not isinstance(after_perf, dict):
        return None
    before_numbers = flatten_numeric_values(before_perf)
    after_numbers = flatten_numeric_values(after_perf)
    deltas = {}
    for key in sorted(set(before_numbers) & set(after_numbers)):
        delta = after_numbers[key] - before_numbers[key]
        if delta:
            deltas[key] = delta
    heartbeat_updated_at_delta_ms = None
    before_updated = before_payload.get("updated_at_ms")
    after_updated = after_payload.get("updated_at_ms")
    if isinstance(before_updated, (int, float)) and isinstance(after_updated, (int, float)):
        heartbeat_updated_at_delta_ms = after_updated - before_updated
    return {
        "heartbeat_updated_at_delta_ms": heartbeat_updated_at_delta_ms,
        "performance_numeric_deltas": deltas,
        "note": (
            "Deltas are collected from runtime/business-os-rxdb-peer.status.json "
            "without invoking ctox status."
        ),
    }


def selected_process_pid(report: dict[str, Any], args: argparse.Namespace) -> int | None:
    if args.pid is not None:
        return args.pid
    process = report.get("process")
    if not isinstance(process, dict):
        return None
    pid_resolution = process.get("pid_resolution")
    if not isinstance(pid_resolution, dict):
        return None
    pid = pid_resolution.get("pid")
    return pid if isinstance(pid, int) else None


def heartbeat_snapshot_age_ms(snapshot: dict[str, Any]) -> int | None:
    payload = snapshot.get("payload") if isinstance(snapshot, dict) else None
    if not isinstance(payload, dict):
        return None
    updated_at_ms = payload.get("updated_at_ms")
    read_at_epoch_ms = snapshot.get("read_at_epoch_ms")
    if not isinstance(updated_at_ms, (int, float)) or isinstance(updated_at_ms, bool):
        return None
    if not isinstance(read_at_epoch_ms, (int, float)) or isinstance(read_at_epoch_ms, bool):
        read_at_epoch_ms = epoch_ms()
    return max(0, int(read_at_epoch_ms) - int(updated_at_ms))


def validate_native_peer_heartbeat_health(
    report: dict[str, Any],
    args: argparse.Namespace,
    max_age_ms: int | None,
    failures: list[dict[str, Any]],
    warnings: list[str],
) -> None:
    if max_age_ms is None:
        return
    heartbeat = report.get("native_peer_heartbeat")
    if not isinstance(heartbeat, dict):
        add_threshold_failure(
            failures,
            metric="native_peer_heartbeat",
            actual=None,
            limit="present",
            message="native peer heartbeat snapshots are unavailable",
        )
        return
    if heartbeat.get("skipped") is True:
        add_threshold_failure(
            failures,
            metric="native_peer_heartbeat.skipped",
            actual=True,
            limit=False,
            message="native peer heartbeat snapshots were skipped",
        )
        return

    expected_pid = selected_process_pid(report, args)
    seen_peer_session_ids: set[str] = set()
    for label in ("before", "after"):
        snapshot = heartbeat.get(label)
        if not isinstance(snapshot, dict):
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}",
                actual=None,
                limit="present",
                message=f"native peer heartbeat {label} snapshot is unavailable",
            )
            continue
        if snapshot.get("exists") is not True:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.exists",
                actual=snapshot.get("exists"),
                limit=True,
                message=f"native peer heartbeat {label} file is missing",
            )
            continue
        if snapshot.get("error"):
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.error",
                actual=snapshot.get("error"),
                limit=None,
                message=f"native peer heartbeat {label} file could not be read",
            )
            continue
        payload = snapshot.get("payload")
        if not isinstance(payload, dict):
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload",
                actual=payload,
                limit="object",
                message=f"native peer heartbeat {label} payload is unavailable",
            )
            continue

        version = payload.get("version")
        if version != EXPECTED_NATIVE_PEER_STATUS_VERSION:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.version",
                actual=version,
                limit=EXPECTED_NATIVE_PEER_STATUS_VERSION,
                message=f"native peer heartbeat {label} has unexpected schema version",
            )
        running = payload.get("running")
        if running is not True:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.running",
                actual=running,
                limit=True,
                message=f"native peer heartbeat {label} does not report running=true",
            )
        replication_up = payload.get("replicationUp")
        if replication_up is not True:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.replicationUp",
                actual=replication_up,
                limit=True,
                message=f"native peer heartbeat {label} does not report replicationUp=true",
            )
        heartbeat_pid = payload.get("pid")
        if expected_pid is not None and heartbeat_pid != expected_pid:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.pid",
                actual=heartbeat_pid,
                limit=expected_pid,
                message=f"native peer heartbeat {label} belongs to a different process",
            )
        peer_session_id = payload.get("peer_session_id")
        if isinstance(peer_session_id, str) and peer_session_id.strip():
            seen_peer_session_ids.add(peer_session_id)
        else:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.peer_session_id",
                actual=peer_session_id,
                limit="non-empty",
                message=f"native peer heartbeat {label} has no peer_session_id",
            )
        database_path = payload.get("database_path")
        if not isinstance(database_path, str) or not database_path.strip():
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.database_path",
                actual=database_path,
                limit="non-empty",
                message=f"native peer heartbeat {label} has no database_path",
            )

        age_ms = heartbeat_snapshot_age_ms(snapshot)
        snapshot["heartbeat_age_ms"] = age_ms
        check_numeric_limit(
            failures,
            warnings,
            metric=f"native_peer_heartbeat.{label}.heartbeat_age_ms",
            actual=age_ms,
            limit=max_age_ms,
            missing_message=f"native peer heartbeat {label} age is unavailable",
        )

        performance = payload.get("performance")
        if not isinstance(performance, dict):
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.performance",
                actual=performance,
                limit="object",
                message=f"native peer heartbeat {label} performance counters are unavailable",
            )
            continue
        performance_schema = performance.get("schema")
        if performance_schema != EXPECTED_NATIVE_PEER_PERFORMANCE_SCHEMA:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.performance.schema",
                actual=performance_schema,
                limit=EXPECTED_NATIVE_PEER_PERFORMANCE_SCHEMA,
                message=f"native peer heartbeat {label} performance schema is unexpected",
            )
        sqlite_schema = (
            performance.get("rxdb_sqlite", {}).get("schema")
            if isinstance(performance.get("rxdb_sqlite"), dict)
            else None
        )
        if sqlite_schema != EXPECTED_NATIVE_PEER_SQLITE_COUNTER_SCHEMA:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.performance.rxdb_sqlite.schema",
                actual=sqlite_schema,
                limit=EXPECTED_NATIVE_PEER_SQLITE_COUNTER_SCHEMA,
                message=f"native peer heartbeat {label} SQLite counter schema is unexpected",
            )
        subjects_schema = (
            performance.get("rxdb_subjects", {}).get("schema")
            if isinstance(performance.get("rxdb_subjects"), dict)
            else None
        )
        if subjects_schema != EXPECTED_NATIVE_PEER_SUBJECT_COUNTER_SCHEMA:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.{label}.payload.performance.rxdb_subjects.schema",
                actual=subjects_schema,
                limit=EXPECTED_NATIVE_PEER_SUBJECT_COUNTER_SCHEMA,
                message=f"native peer heartbeat {label} subject counter schema is unexpected",
            )

    if len(seen_peer_session_ids) > 1:
        add_threshold_failure(
            failures,
            metric="native_peer_heartbeat.peer_session_id",
            actual=sorted(seen_peer_session_ids),
            limit="unchanged",
            message="native peer heartbeat peer_session_id changed during sampling",
        )


def service_performance_status_delta(
    before: dict[str, Any] | None,
    after: dict[str, Any] | None,
) -> dict[str, Any] | None:
    if not before or not after:
        return None
    before_payload = before.get("payload")
    after_payload = after.get("payload")
    if not isinstance(before_payload, dict) or not isinstance(after_payload, dict):
        return None
    before_perf = before_payload.get("performance")
    after_perf = after_payload.get("performance")
    if not isinstance(before_perf, dict) or not isinstance(after_perf, dict):
        return None
    before_numbers = flatten_numeric_values(before_perf)
    after_numbers = flatten_numeric_values(after_perf)
    deltas = {}
    for key in sorted(set(before_numbers) & set(after_numbers)):
        delta = after_numbers[key] - before_numbers[key]
        if delta:
            deltas[key] = delta
    return {
        "performance_numeric_deltas": deltas,
        "note": (
            "Deltas are collected from runtime/service-performance.status.json "
            "without invoking ctox status."
        ),
    }


def service_status_performance_delta(samples: list[dict[str, Any]]) -> dict[str, Any] | None:
    first_perf = next(
        (
            sample.get("performance")
            for sample in samples
            if isinstance(sample.get("performance"), dict)
        ),
        None,
    )
    last_perf = next(
        (
            sample.get("performance")
            for sample in reversed(samples)
            if isinstance(sample.get("performance"), dict)
        ),
        None,
    )
    if not isinstance(first_perf, dict) or not isinstance(last_perf, dict):
        return None

    before_numbers = flatten_numeric_values(first_perf)
    after_numbers = flatten_numeric_values(last_perf)
    deltas = {}
    for key in sorted(set(before_numbers) & set(after_numbers)):
        delta = after_numbers[key] - before_numbers[key]
        if delta:
            deltas[key] = delta
    return {
        "performance_numeric_deltas": deltas,
        "note": (
            "Deltas are collected from separate ctox status samples after "
            "process CPU sampling."
        ),
    }


def pgrep_candidates(process_name: str) -> dict[str, Any]:
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
        "process_name": process_name,
        "source": "pgrep -x",
        "candidates": candidates,
        "returncode": result.returncode,
        "stderr": result.stderr.strip() or None,
    }


def resolve_pid(pid: int | None, process_name: str) -> dict[str, Any]:
    inventory = pgrep_candidates(process_name)
    candidates = [
        candidate for candidate in inventory.get("candidates", []) if isinstance(candidate, int)
    ]
    if pid is not None:
        all_candidates = sorted(set(candidates + [pid]))
        return {
            "pid": pid,
            "process_name": process_name,
            "source": "argument",
            "candidates": all_candidates,
            "pgrep": inventory,
            "extra_candidate_pids": [candidate for candidate in all_candidates if candidate != pid],
        }

    selected = max(candidates) if candidates else None
    return {
        "pid": selected,
        "process_name": process_name,
        "source": inventory.get("source"),
        "candidates": candidates,
        "returncode": inventory.get("returncode"),
        "stderr": inventory.get("stderr"),
        "extra_candidate_pids": [
            candidate for candidate in candidates if selected is not None and candidate != selected
        ],
        "error": inventory.get("error"),
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


def process_inventory_snapshot() -> dict[str, Any]:
    result = subprocess.run(
        ["ps", "axo", "pid=,ppid=,pgid=,pcpu=,comm="],
        text=True,
        capture_output=True,
        timeout=5,
        check=False,
    )
    report: dict[str, Any] = {
        "at": utc_now(),
        "returncode": result.returncode,
        "processes": [],
    }
    if result.returncode != 0:
        report["error"] = (result.stderr.strip() or "process inventory failed")[:400]
        return report
    processes = []
    for line in result.stdout.splitlines():
        parts = line.strip().split(None, 4)
        if len(parts) < 5:
            continue
        pid_value = int_or_none(parts[0])
        ppid_value = int_or_none(parts[1])
        pgid_value = int_or_none(parts[2])
        cpu_value = float_or_none(parts[3])
        if pid_value is None:
            continue
        processes.append(
            {
                "pid": pid_value,
                "ppid": ppid_value,
                "pgid": pgid_value,
                "cpu_percent": cpu_value,
                "comm": parts[4],
            }
        )
    report["processes"] = processes
    return report


def descendants_by_ppid(processes: list[dict[str, Any]], root_pid: int) -> set[int]:
    children_by_parent: dict[int, list[int]] = {}
    for process in processes:
        pid_value = process.get("pid")
        ppid_value = process.get("ppid")
        if isinstance(pid_value, int) and isinstance(ppid_value, int):
            children_by_parent.setdefault(ppid_value, []).append(pid_value)
    descendants: set[int] = set()
    stack = list(children_by_parent.get(root_pid, []))
    while stack:
        child = stack.pop()
        if child in descendants:
            continue
        descendants.add(child)
        stack.extend(children_by_parent.get(child, []))
    return descendants


def process_scope_snapshot(selected_pid: int, candidate_pids: list[int]) -> dict[str, Any]:
    inventory = process_inventory_snapshot()
    processes = inventory.get("processes")
    if not isinstance(processes, list):
        return {
            "inventory": inventory,
            "scope_pids": [selected_pid],
            "selected_pid": selected_pid,
            "cpu_percent_total": None,
            "processes": [],
            "error": "process inventory unavailable",
        }
    by_pid = {
        process.get("pid"): process
        for process in processes
        if isinstance(process, dict) and isinstance(process.get("pid"), int)
    }
    selected = by_pid.get(selected_pid)
    selected_pgid = selected.get("pgid") if isinstance(selected, dict) else None
    scope_pids = set(candidate_pids)
    scope_pids.add(selected_pid)
    if isinstance(selected_pgid, int):
        for process in processes:
            if isinstance(process, dict) and process.get("pgid") == selected_pgid:
                pid_value = process.get("pid")
                if isinstance(pid_value, int):
                    scope_pids.add(pid_value)
    scope_pids.update(descendants_by_ppid(processes, selected_pid))
    scoped = [
        process
        for pid_value, process in sorted(by_pid.items())
        if isinstance(pid_value, int) and pid_value in scope_pids
    ]
    cpu_values = [
        float(process["cpu_percent"])
        for process in scoped
        if isinstance(process.get("cpu_percent"), (int, float))
        and not isinstance(process.get("cpu_percent"), bool)
    ]
    return {
        "inventory": {
            "at": inventory.get("at"),
            "returncode": inventory.get("returncode"),
            "process_count": len(processes),
            "error": inventory.get("error"),
        },
        "selected_pid": selected_pid,
        "selected_pgid": selected_pgid,
        "scope_pids": sorted(scope_pids),
        "scope_process_count": len(scoped),
        "candidate_pids": candidate_pids,
        "processes": scoped,
        "cpu_percent_total": sum(cpu_values) if cpu_values else None,
        "note": (
            "Scope includes all ctox-real candidates, the selected process group, "
            "and descendants of the selected PID."
        ),
    }


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

    candidate_pids = sorted(
        {
            candidate
            for candidate in resolved.get("candidates", [])
            if isinstance(candidate, int) and candidate > 0
        }
    )
    if isinstance(selected_pid, int) and selected_pid > 0:
        candidate_pids = sorted(set(candidate_pids + [int(selected_pid)]))

    samples = []
    for index in range(count):
        candidate_samples = [read_process_sample(candidate) for candidate in candidate_pids]
        scope_sample = process_scope_snapshot(int(selected_pid), candidate_pids)
        selected_sample = next(
            (
                sample
                for sample in candidate_samples
                if sample.get("pid") == int(selected_pid)
            ),
            None,
        )
        if selected_sample is None:
            selected_sample = read_process_sample(int(selected_pid))
            candidate_samples.append(selected_sample)
        selected_sample = dict(selected_sample)
        aggregate_cpu = sum(
            float(sample["cpu_percent"])
            for sample in candidate_samples
            if isinstance(sample.get("cpu_percent"), (int, float))
            and not isinstance(sample.get("cpu_percent"), bool)
        )
        selected_sample["candidate_samples"] = candidate_samples
        selected_sample["candidate_cpu_percent_total"] = aggregate_cpu
        selected_sample["candidate_pids"] = candidate_pids
        selected_sample["scope_sample"] = scope_sample
        samples.append(selected_sample)
        if index + 1 < count and interval > 0:
            time.sleep(interval)

    cpu_values = [
        sample["cpu_percent"]
        for sample in samples
        if isinstance(sample.get("cpu_percent"), (int, float))
    ]
    aggregate_cpu_values = [
        sample["candidate_cpu_percent_total"]
        for sample in samples
        if isinstance(sample.get("candidate_cpu_percent_total"), (int, float))
        and not isinstance(sample.get("candidate_cpu_percent_total"), bool)
    ]
    scope_cpu_values = [
        sample.get("scope_sample", {}).get("cpu_percent_total")
        for sample in samples
        if isinstance(sample.get("scope_sample"), dict)
        and isinstance(sample.get("scope_sample", {}).get("cpu_percent_total"), (int, float))
        and not isinstance(sample.get("scope_sample", {}).get("cpu_percent_total"), bool)
    ]
    first = samples[0] if samples else {}
    last = samples[-1] if samples else {}
    last_scope = last.get("scope_sample") if isinstance(last.get("scope_sample"), dict) else {}
    summary = {
        "sample_count": len(samples),
        "interval_seconds": interval,
        "candidate_count": len(candidate_pids),
        "candidate_pids": candidate_pids,
        "extra_candidate_pids": resolved.get("extra_candidate_pids", []),
        "cpu_percent_avg": statistics.fmean(cpu_values) if cpu_values else None,
        "cpu_percent_min": min(cpu_values) if cpu_values else None,
        "cpu_percent_max": max(cpu_values) if cpu_values else None,
        "cpu_percent_p95": percentile(cpu_values, 0.95),
        "candidate_cpu_percent_total_avg": (
            statistics.fmean(aggregate_cpu_values) if aggregate_cpu_values else None
        ),
        "candidate_cpu_percent_total_max": (
            max(aggregate_cpu_values) if aggregate_cpu_values else None
        ),
        "candidate_cpu_percent_total_p95": percentile(aggregate_cpu_values, 0.95),
        "scope_pids": last_scope.get("scope_pids") if isinstance(last_scope, dict) else [],
        "scope_process_count": (
            last_scope.get("scope_process_count") if isinstance(last_scope, dict) else None
        ),
        "scope_cpu_percent_total_avg": (
            statistics.fmean(scope_cpu_values) if scope_cpu_values else None
        ),
        "scope_cpu_percent_total_max": max(scope_cpu_values) if scope_cpu_values else None,
        "scope_cpu_percent_total_p95": percentile(scope_cpu_values, 0.95),
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
            sample = {
                "at": utc_now(),
                "latency_ms": elapsed_ms,
                "returncode": result.returncode,
                "stdout_bytes": len(result.stdout.encode("utf-8", errors="replace")),
                "stderr_tail": result.stderr[-1000:] or None,
                "json_ok": parsed_json is not None,
                "status_ok": parsed_json.get("ok") if isinstance(parsed_json, dict) else None,
            }
            performance = (
                parsed_json.get("performance") if isinstance(parsed_json, dict) else None
            )
            if isinstance(performance, dict):
                sample["performance"] = performance
            samples.append(sample)
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
        "performance_delta": service_status_performance_delta(samples),
        "note": "Status latency is sampled after process CPU sampling.",
    }


def database_specs(root: Path, extras: list[str]) -> list[tuple[str, Path]]:
    specs = [(name, root / relative) for name, relative in DEFAULT_DATABASES]
    runtime_dir = root / "runtime"
    if runtime_dir.exists():
        discovered = sorted(
            {
                path
                for pattern in ("*.sqlite3", "*.db")
                for path in runtime_dir.glob(pattern)
                if path.is_file()
            }
        )
        for path in discovered:
            specs.append((f"runtime:{path.name}", path))
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
    deduped: list[tuple[str, Path]] = []
    seen: set[Path] = set()
    for name, path in specs:
        key = path.expanduser().resolve() if path.exists() else path.expanduser().absolute()
        if key in seen:
            continue
        seen.add(key)
        deduped.append((name, path))
    return deduped


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


def metric_segment(value: Any) -> str:
    text = str(value) if value is not None else "unknown"
    segment = re.sub(r"[^A-Za-z0-9_-]+", "_", text).strip("_")
    return segment or "unknown"


def record_numeric_metric(metrics: dict[str, float], key: str, value: Any) -> None:
    if isinstance(value, bool):
        return
    if isinstance(value, (int, float)):
        metrics[key] = float(value)


def database_metric_numbers(databases: list[dict[str, Any]]) -> dict[str, float]:
    metrics: dict[str, float] = {}
    for database in databases:
        if not isinstance(database, dict):
            continue
        prefix = metric_segment(database.get("name") or database.get("path"))
        for key in (
            "page_count",
            "database_page_bytes",
            "freelist_count",
            "freelist_bytes",
            "table_count",
        ):
            record_numeric_metric(metrics, f"{prefix}.{key}", database.get(key))
        dbstat = database.get("dbstat")
        objects = dbstat.get("objects") if isinstance(dbstat, dict) else None
        if isinstance(objects, list):
            for item in objects:
                if not isinstance(item, dict):
                    continue
                object_prefix = f"{prefix}.dbstat.{metric_segment(item.get('name'))}"
                record_numeric_metric(metrics, f"{object_prefix}.bytes", item.get("bytes"))
                record_numeric_metric(metrics, f"{object_prefix}.pages", item.get("pages"))
        collections = database.get("rxdb_collections")
        if isinstance(collections, list):
            for item in collections:
                if not isinstance(item, dict):
                    continue
                collection_prefix = (
                    f"{prefix}.rxdb_collections.{metric_segment(item.get('collection'))}"
                )
                for key in ("row_count", "data_bytes", "tombstone_count"):
                    record_numeric_metric(metrics, f"{collection_prefix}.{key}", item.get(key))
        chunks = database.get("desktop_file_chunks")
        if isinstance(chunks, dict):
            chunk_prefix = f"{prefix}.desktop_file_chunks"
            for key in (
                "row_count",
                "sampled_rows",
                "file_count",
                "live_generation_count",
                "live_bytes_sampled",
                "retained_generation_count",
                "retained_bytes_sampled",
                "stale_generation_count",
                "stale_bytes_sampled",
                "tombstone_rows_sampled",
                "tombstone_bytes_sampled",
                "malformed_rows_sampled",
            ):
                record_numeric_metric(metrics, f"{chunk_prefix}.{key}", chunks.get(key))
    return metrics


def database_metric_snapshot(
    root: Path,
    extras: list[str],
    *,
    max_tables: int,
    max_dbstat_rows: int,
    max_chunk_rows: int,
    retain_chunk_generations: int,
) -> dict[str, Any]:
    databases = inspect_databases(
        root,
        extras,
        max_tables=max_tables,
        max_dbstat_rows=max_dbstat_rows,
        max_chunk_rows=max_chunk_rows,
        retain_chunk_generations=retain_chunk_generations,
    )
    return {
        "at": utc_now(),
        "databases": databases,
        "numeric_values": database_metric_numbers(databases),
        "note": (
            "Database metric snapshots are read-only diagnostics around process "
            "CPU sampling."
        ),
    }


def database_metric_delta(
    before: dict[str, Any] | None,
    after: dict[str, Any] | None,
) -> dict[str, Any] | None:
    if not isinstance(before, dict) or not isinstance(after, dict):
        return None
    before_values = before.get("numeric_values")
    after_values = after.get("numeric_values")
    if not isinstance(before_values, dict) or not isinstance(after_values, dict):
        return None
    deltas = {}
    for key in sorted(set(before_values) | set(after_values)):
        before_value = before_values.get(key, 0.0)
        after_value = after_values.get(key, 0.0)
        if (
            isinstance(before_value, (int, float))
            and not isinstance(before_value, bool)
            and isinstance(after_value, (int, float))
            and not isinstance(after_value, bool)
        ):
            delta = float(after_value) - float(before_value)
            if delta:
                deltas[key] = delta
    return {
        "before_at": before.get("at"),
        "after_at": after.get("at"),
        "numeric_deltas": deltas,
    }


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


def assertion_limit(
    value: float | int | None,
    default: float | int,
    enabled: bool,
) -> float | int | None:
    if value is not None:
        return value
    if enabled:
        return default
    return None


def add_threshold_failure(
    failures: list[dict[str, Any]],
    *,
    metric: str,
    actual: Any,
    limit: float | int,
    message: str,
) -> None:
    failures.append(
        {
            "metric": metric,
            "actual": actual,
            "limit": limit,
            "message": message,
        }
    )


def check_numeric_limit(
    failures: list[dict[str, Any]],
    warnings: list[str],
    *,
    metric: str,
    actual: Any,
    limit: float | int | None,
    missing_message: str,
) -> None:
    if limit is None:
        return
    if not isinstance(actual, (int, float)) or isinstance(actual, bool):
        warnings.append(missing_message)
        add_threshold_failure(
            failures,
            metric=metric,
            actual=actual,
            limit=limit,
            message=missing_message,
        )
        return
    if actual > limit:
        add_threshold_failure(
            failures,
            metric=metric,
            actual=actual,
            limit=limit,
            message=f"{metric} exceeded configured limit",
        )


def heartbeat_delta_thresholds(args: argparse.Namespace) -> list[tuple[str, float]]:
    thresholds = list(DEFAULT_ASSERT_HEARTBEAT_DELTAS) if args.assert_idle else []
    thresholds.extend(parse_metric_threshold(raw) for raw in args.max_heartbeat_delta)
    return thresholds


def database_metric_delta_thresholds(args: argparse.Namespace) -> list[tuple[str, float]]:
    thresholds = (
        list(DEFAULT_ASSERT_DB_METRIC_DELTAS)
        if args.assert_idle and not args.skip_db
        else []
    )
    thresholds.extend(parse_metric_threshold(raw) for raw in args.max_db_metric_delta)
    return thresholds


def service_status_delta_thresholds(args: argparse.Namespace) -> list[tuple[str, float]]:
    thresholds = (
        list(DEFAULT_ASSERT_SERVICE_STATUS_DELTAS)
        if args.assert_idle and not args.skip_status
        else []
    )
    thresholds.extend(parse_metric_threshold(raw) for raw in args.max_service_status_delta)
    return thresholds


def service_performance_delta_thresholds(args: argparse.Namespace) -> list[tuple[str, float]]:
    thresholds = (
        list(DEFAULT_ASSERT_SERVICE_PERFORMANCE_DELTAS)
        if args.assert_idle and not args.skip_service_performance
        else []
    )
    thresholds.extend(parse_metric_threshold(raw) for raw in args.max_service_performance_delta)
    return thresholds


def sync_run_delta_thresholds(args: argparse.Namespace) -> list[tuple[str, float]]:
    thresholds = list(DEFAULT_ASSERT_SYNC_RUN_DELTAS) if args.assert_idle else []
    thresholds.extend(parse_metric_threshold(raw) for raw in args.max_sync_run_delta)
    return thresholds


def evaluate_assertions(report: dict[str, Any], args: argparse.Namespace) -> dict[str, Any]:
    heartbeat_thresholds = heartbeat_delta_thresholds(args)
    db_metric_thresholds = database_metric_delta_thresholds(args)
    service_status_thresholds = service_status_delta_thresholds(args)
    service_performance_thresholds = service_performance_delta_thresholds(args)
    sync_run_thresholds = sync_run_delta_thresholds(args)
    heartbeat_max_age_ms = assertion_limit(
        args.max_heartbeat_age_ms,
        DEFAULT_ASSERT_HEARTBEAT_MAX_AGE_MS,
        args.assert_idle and not args.skip_heartbeat,
    )
    enabled = bool(
        args.assert_idle
        or heartbeat_thresholds
        or heartbeat_max_age_ms is not None
        or db_metric_thresholds
        or service_status_thresholds
        or service_performance_thresholds
        or sync_run_thresholds
    )
    cpu_avg_limit = assertion_limit(args.max_cpu_avg, DEFAULT_ASSERT_CPU_AVG, args.assert_idle)
    cpu_p95_limit = assertion_limit(args.max_cpu_p95, DEFAULT_ASSERT_CPU_P95, args.assert_idle)
    status_p95_limit = assertion_limit(
        args.max_status_p95_ms,
        DEFAULT_ASSERT_STATUS_P95_MS,
        args.assert_idle and not args.skip_status,
    )
    db_growth_limit = assertion_limit(
        args.max_db_growth_bytes,
        DEFAULT_ASSERT_DB_GROWTH_BYTES,
        args.assert_idle,
    )
    db_file_growth_limit = assertion_limit(
        args.max_db_file_growth_bytes,
        DEFAULT_ASSERT_DB_FILE_GROWTH_BYTES,
        args.assert_idle,
    )
    if any(
        value is not None
        for value in (
            cpu_avg_limit,
            cpu_p95_limit,
            args.max_cpu_max,
            status_p95_limit,
            db_growth_limit,
            db_file_growth_limit,
        )
    ):
        enabled = True

    failures: list[dict[str, Any]] = []
    warnings: list[str] = []
    thresholds: dict[str, Any] = {
        "max_cpu_avg": cpu_avg_limit,
        "max_cpu_p95": cpu_p95_limit,
        "max_cpu_max": args.max_cpu_max,
        "max_status_p95_ms": status_p95_limit,
        "max_db_growth_bytes": db_growth_limit,
        "max_db_file_growth_bytes": db_file_growth_limit,
        "max_heartbeat_age_ms": heartbeat_max_age_ms,
        "max_heartbeat_deltas": [
            {"pattern": pattern, "limit": limit} for pattern, limit in heartbeat_thresholds
        ],
        "max_database_metric_deltas": [
            {"pattern": pattern, "limit": limit}
            for pattern, limit in db_metric_thresholds
        ],
        "max_service_status_deltas": [
            {"pattern": pattern, "limit": limit}
            for pattern, limit in service_status_thresholds
        ],
        "max_service_performance_deltas": [
            {"pattern": pattern, "limit": limit}
            for pattern, limit in service_performance_thresholds
        ],
        "max_sync_run_deltas": [
            {"pattern": pattern, "limit": limit} for pattern, limit in sync_run_thresholds
        ],
    }

    process = report.get("process") if isinstance(report.get("process"), dict) else {}
    process_summary = process.get("summary") if isinstance(process.get("summary"), dict) else {}
    check_numeric_limit(
        failures,
        warnings,
        metric="process.summary.cpu_percent_avg",
        actual=process_summary.get("cpu_percent_avg"),
        limit=cpu_avg_limit,
        missing_message="process CPU average is unavailable",
    )
    check_numeric_limit(
        failures,
        warnings,
        metric="process.summary.cpu_percent_p95",
        actual=process_summary.get("cpu_percent_p95"),
        limit=cpu_p95_limit,
        missing_message="process CPU p95 is unavailable",
    )
    check_numeric_limit(
        failures,
        warnings,
        metric="process.summary.cpu_percent_max",
        actual=process_summary.get("cpu_percent_max"),
        limit=args.max_cpu_max,
        missing_message="process CPU max is unavailable",
    )
    check_numeric_limit(
        failures,
        warnings,
        metric="process.summary.candidate_cpu_percent_total_avg",
        actual=process_summary.get("candidate_cpu_percent_total_avg"),
        limit=cpu_avg_limit,
        missing_message="aggregate candidate process CPU average is unavailable",
    )
    check_numeric_limit(
        failures,
        warnings,
        metric="process.summary.candidate_cpu_percent_total_p95",
        actual=process_summary.get("candidate_cpu_percent_total_p95"),
        limit=cpu_p95_limit,
        missing_message="aggregate candidate process CPU p95 is unavailable",
    )
    check_numeric_limit(
        failures,
        warnings,
        metric="process.summary.candidate_cpu_percent_total_max",
        actual=process_summary.get("candidate_cpu_percent_total_max"),
        limit=args.max_cpu_max,
        missing_message="aggregate candidate process CPU max is unavailable",
    )
    check_numeric_limit(
        failures,
        warnings,
        metric="process.summary.scope_cpu_percent_total_avg",
        actual=process_summary.get("scope_cpu_percent_total_avg"),
        limit=cpu_avg_limit,
        missing_message="process scope CPU average is unavailable",
    )
    check_numeric_limit(
        failures,
        warnings,
        metric="process.summary.scope_cpu_percent_total_p95",
        actual=process_summary.get("scope_cpu_percent_total_p95"),
        limit=cpu_p95_limit,
        missing_message="process scope CPU p95 is unavailable",
    )
    check_numeric_limit(
        failures,
        warnings,
        metric="process.summary.scope_cpu_percent_total_max",
        actual=process_summary.get("scope_cpu_percent_total_max"),
        limit=args.max_cpu_max,
        missing_message="process scope CPU max is unavailable",
    )
    extra_candidate_pids = process_summary.get("extra_candidate_pids")
    if args.assert_idle and isinstance(extra_candidate_pids, list) and extra_candidate_pids:
        add_threshold_failure(
            failures,
            metric="process.summary.extra_candidate_pids",
            actual=extra_candidate_pids,
            limit=[],
            message="extra ctox-real candidates were present during idle sampling",
        )

    status = report.get("status_latency") if isinstance(report.get("status_latency"), dict) else {}
    status_summary = status.get("summary") if isinstance(status.get("summary"), dict) else {}
    check_numeric_limit(
        failures,
        warnings,
        metric="status_latency.summary.latency_ms_p95",
        actual=status_summary.get("latency_ms_p95"),
        limit=status_p95_limit,
        missing_message="status latency p95 is unavailable",
    )
    status_performance_delta = (
        status.get("performance_delta") if isinstance(status.get("performance_delta"), dict) else None
    )
    service_performance_deltas = (
        status_performance_delta.get("performance_numeric_deltas")
        if isinstance(status_performance_delta, dict)
        else None
    )
    if isinstance(service_performance_deltas, dict):
        for pattern, limit in service_status_thresholds:
            matches = [
                (key, value)
                for key, value in service_performance_deltas.items()
                if fnmatch.fnmatchcase(key, pattern)
            ]
            if not matches:
                warnings.append(f"service status delta pattern matched no metrics: {pattern}")
                add_threshold_failure(
                    failures,
                    metric=f"status_latency.performance_delta.performance_numeric_deltas.{pattern}",
                    actual=None,
                    limit=limit,
                    message="service status delta pattern matched no metrics",
                )
                continue
            for key, value in matches:
                if isinstance(value, (int, float)) and not isinstance(value, bool) and value > limit:
                    add_threshold_failure(
                        failures,
                        metric=f"status_latency.performance_delta.performance_numeric_deltas.{key}",
                        actual=value,
                        limit=limit,
                        message=f"service status delta {key} exceeded configured limit",
                    )
    elif service_status_thresholds:
        warnings.append("service status performance deltas are unavailable")
        for pattern, limit in service_status_thresholds:
            add_threshold_failure(
                failures,
                metric=f"status_latency.performance_delta.performance_numeric_deltas.{pattern}",
                actual=None,
                limit=limit,
                message="service status performance deltas are unavailable",
            )

    service_performance = (
        report.get("service_performance_status")
        if isinstance(report.get("service_performance_status"), dict)
        else {}
    )
    service_file_delta = (
        service_performance.get("delta")
        if isinstance(service_performance.get("delta"), dict)
        else None
    )
    service_file_deltas = (
        service_file_delta.get("performance_numeric_deltas")
        if isinstance(service_file_delta, dict)
        else None
    )
    if isinstance(service_file_deltas, dict):
        before = service_performance.get("before")
        after = service_performance.get("after")
        before_payload = before.get("payload") if isinstance(before, dict) else None
        after_payload = after.get("payload") if isinstance(after, dict) else None
        before_perf = (
            before_payload.get("performance") if isinstance(before_payload, dict) else {}
        )
        after_perf = after_payload.get("performance") if isinstance(after_payload, dict) else {}
        before_numbers = flatten_numeric_values(before_perf)
        after_numbers = flatten_numeric_values(after_perf)
        all_numbers = set(before_numbers) | set(after_numbers) | set(service_file_deltas)
        before_process = (
            before_perf.get("process") if isinstance(before_perf, dict) else None
        )
        after_process = after_perf.get("process") if isinstance(after_perf, dict) else None
        if service_performance_thresholds:
            if not isinstance(before_process, dict) or not isinstance(after_process, dict):
                add_threshold_failure(
                    failures,
                    metric="service_performance_status.performance.process",
                    actual=None,
                    limit="present",
                    message="service performance process identity is unavailable",
                )
            else:
                before_pid = before_process.get("pid")
                after_pid = after_process.get("pid")
                if args.pid is not None and before_pid != args.pid:
                    add_threshold_failure(
                        failures,
                        metric="service_performance_status.before.performance.process.pid",
                        actual=before_pid,
                        limit=args.pid,
                        message="service performance artifact belongs to a different process",
                    )
                if args.pid is not None and after_pid != args.pid:
                    add_threshold_failure(
                        failures,
                        metric="service_performance_status.after.performance.process.pid",
                        actual=after_pid,
                        limit=args.pid,
                        message="service performance artifact belongs to a different process",
                    )
                if before_process.get("boot_id") != after_process.get("boot_id"):
                    add_threshold_failure(
                        failures,
                        metric="service_performance_status.performance.process.boot_id",
                        actual={
                            "before": before_process.get("boot_id"),
                            "after": after_process.get("boot_id"),
                        },
                        limit="unchanged",
                        message="service performance artifact changed process boot identity",
                    )
        for pattern, limit in service_performance_thresholds:
            candidate_keys = {key for key in all_numbers if fnmatch.fnmatchcase(key, pattern)}
            if not candidate_keys:
                warnings.append(f"service performance delta pattern matched no metrics: {pattern}")
                add_threshold_failure(
                    failures,
                    metric=f"service_performance_status.delta.performance_numeric_deltas.{pattern}",
                    actual=None,
                    limit=limit,
                    message="service performance delta pattern matched no metrics",
                )
                continue
            for key in sorted(candidate_keys):
                value = service_file_deltas.get(key, 0.0)
                if not isinstance(value, (int, float)) or isinstance(value, bool):
                    continue
                if value < 0:
                    add_threshold_failure(
                        failures,
                        metric=f"service_performance_status.delta.performance_numeric_deltas.{key}",
                        actual=value,
                        limit="non-negative",
                        message=f"service performance counter {key} reset during sampling",
                    )
                elif limit == 0.0 and value != 0.0:
                    add_threshold_failure(
                        failures,
                        metric=f"service_performance_status.delta.performance_numeric_deltas.{key}",
                        actual=value,
                        limit=limit,
                        message=f"service performance delta {key} changed during passive idle",
                    )
                elif value > limit:
                    add_threshold_failure(
                        failures,
                        metric=f"service_performance_status.delta.performance_numeric_deltas.{key}",
                        actual=value,
                        limit=limit,
                        message=f"service performance delta {key} exceeded configured limit",
                    )
    elif service_performance_thresholds:
        warnings.append("service performance-file deltas are unavailable")
        for pattern, limit in service_performance_thresholds:
            add_threshold_failure(
                failures,
                metric=f"service_performance_status.delta.performance_numeric_deltas.{pattern}",
                actual=None,
                limit=limit,
                message="service performance-file deltas are unavailable",
            )

    sync_run_delta = report.get("sync_run_delta")
    sync_run_deltas = (
        sync_run_delta.get("numeric_deltas")
        if isinstance(sync_run_delta, dict)
        else None
    )
    if isinstance(sync_run_deltas, dict):
        all_sync_run_numbers = sync_run_snapshot_numbers(
            report.get("sync_run_snapshots", {}).get("after_cpu")
            if isinstance(report.get("sync_run_snapshots"), dict)
            else None
        )
        for pattern, limit in sync_run_thresholds:
            candidate_keys = {
                key for key in all_sync_run_numbers if fnmatch.fnmatchcase(key, pattern)
            }
            candidate_keys.update(
                key for key in sync_run_deltas if fnmatch.fnmatchcase(key, pattern)
            )
            if not candidate_keys:
                warnings.append(f"sync-run delta pattern matched no metrics: {pattern}")
                add_threshold_failure(
                    failures,
                    metric=f"sync_run_delta.numeric_deltas.{pattern}",
                    actual=None,
                    limit=limit,
                    message="sync-run delta pattern matched no metrics",
                )
                continue
            for key in sorted(candidate_keys):
                value = sync_run_deltas.get(key, 0.0)
                if isinstance(value, (int, float)) and not isinstance(value, bool) and value > limit:
                    add_threshold_failure(
                        failures,
                        metric=f"sync_run_delta.numeric_deltas.{key}",
                        actual=value,
                        limit=limit,
                        message=f"sync-run delta {key} exceeded configured limit",
                    )
    elif sync_run_thresholds:
        warnings.append("sync-run deltas are unavailable")
        for pattern, limit in sync_run_thresholds:
            add_threshold_failure(
                failures,
                metric=f"sync_run_delta.numeric_deltas.{pattern}",
                actual=None,
                limit=limit,
                message="sync-run deltas are unavailable",
            )

    db_growth = report.get("database_file_growth")
    total_growth = db_growth.get("total_growth_bytes") if isinstance(db_growth, dict) else None
    check_numeric_limit(
        failures,
        warnings,
        metric="database_file_growth.total_growth_bytes",
        actual=total_growth,
        limit=db_growth_limit,
        missing_message="database file growth is unavailable",
    )
    if db_file_growth_limit is not None:
        growth_items = db_growth.get("databases") if isinstance(db_growth, dict) else None
        if not isinstance(growth_items, list):
            add_threshold_failure(
                failures,
                metric="database_file_growth.databases",
                actual=None,
                limit=db_file_growth_limit,
                message="database file component growth is unavailable",
            )
        else:
            for item in growth_items:
                if not isinstance(item, dict):
                    continue
                name = item.get("name") or item.get("path") or "unknown"
                file_growth = item.get("file_growth_bytes")
                if not isinstance(file_growth, dict):
                    add_threshold_failure(
                        failures,
                        metric=f"database_file_growth.databases.{name}.file_growth_bytes",
                        actual=None,
                        limit=db_file_growth_limit,
                        message="database file component growth is unavailable",
                    )
                    continue
                for component, value in sorted(file_growth.items()):
                    if (
                        isinstance(value, (int, float))
                        and not isinstance(value, bool)
                        and value > db_file_growth_limit
                    ):
                        add_threshold_failure(
                            failures,
                            metric=(
                                f"database_file_growth.databases.{name}."
                                f"file_growth_bytes.{component}"
                            ),
                            actual=value,
                            limit=db_file_growth_limit,
                            message="database file component growth exceeded configured limit",
                        )

    db_metric_delta = report.get("database_metric_delta")
    db_metric_deltas = (
        db_metric_delta.get("numeric_deltas")
        if isinstance(db_metric_delta, dict)
        else None
    )
    if isinstance(db_metric_deltas, dict):
        metric_snapshots = report.get("database_metric_snapshots")
        after_snapshot = (
            metric_snapshots.get("after_cpu")
            if isinstance(metric_snapshots, dict)
            else None
        )
        after_values = (
            after_snapshot.get("numeric_values")
            if isinstance(after_snapshot, dict)
            else {}
        )
        all_numbers = set(after_values) | set(db_metric_deltas)
        for pattern, limit in db_metric_thresholds:
            candidate_keys = {key for key in all_numbers if fnmatch.fnmatchcase(key, pattern)}
            if not candidate_keys:
                warnings.append(f"database metric delta pattern matched no metrics: {pattern}")
                add_threshold_failure(
                    failures,
                    metric=f"database_metric_delta.numeric_deltas.{pattern}",
                    actual=None,
                    limit=limit,
                    message="database metric delta pattern matched no metrics",
                )
                continue
            for key in sorted(candidate_keys):
                value = db_metric_deltas.get(key, 0.0)
                if (
                    isinstance(value, (int, float))
                    and not isinstance(value, bool)
                    and value > limit
                ):
                    add_threshold_failure(
                        failures,
                        metric=f"database_metric_delta.numeric_deltas.{key}",
                        actual=value,
                        limit=limit,
                        message=f"database metric delta {key} exceeded configured limit",
                    )
    elif db_metric_thresholds:
        warnings.append("database metric deltas are unavailable")
        for pattern, limit in db_metric_thresholds:
            add_threshold_failure(
                failures,
                metric=f"database_metric_delta.numeric_deltas.{pattern}",
                actual=None,
                limit=limit,
                message="database metric deltas are unavailable",
            )

    heartbeat = report.get("native_peer_heartbeat")
    validate_native_peer_heartbeat_health(
        report,
        args,
        heartbeat_max_age_ms,
        failures,
        warnings,
    )
    heartbeat_delta = heartbeat.get("delta") if isinstance(heartbeat, dict) else None
    performance_deltas = (
        heartbeat_delta.get("performance_numeric_deltas")
        if isinstance(heartbeat_delta, dict)
        else None
    )
    if isinstance(performance_deltas, dict):
        before = heartbeat.get("before") if isinstance(heartbeat, dict) else None
        after = heartbeat.get("after") if isinstance(heartbeat, dict) else None
        before_payload = before.get("payload") if isinstance(before, dict) else None
        after_payload = after.get("payload") if isinstance(after, dict) else None
        before_perf = (
            before_payload.get("performance") if isinstance(before_payload, dict) else {}
        )
        after_perf = after_payload.get("performance") if isinstance(after_payload, dict) else {}
        before_numbers = flatten_numeric_values(before_perf)
        after_numbers = flatten_numeric_values(after_perf)
        all_numbers = set(before_numbers) | set(after_numbers) | set(performance_deltas)
        for pattern, limit in heartbeat_thresholds:
            candidate_keys = {key for key in all_numbers if fnmatch.fnmatchcase(key, pattern)}
            if not candidate_keys:
                warnings.append(f"heartbeat delta pattern matched no metrics: {pattern}")
                continue
            for key in sorted(candidate_keys):
                value = performance_deltas.get(key, 0.0)
                if isinstance(value, (int, float)) and not isinstance(value, bool) and value > limit:
                    add_threshold_failure(
                        failures,
                        metric=f"native_peer_heartbeat.delta.performance_numeric_deltas.{key}",
                        actual=value,
                        limit=limit,
                        message=f"heartbeat delta {key} exceeded configured limit",
                    )
    elif heartbeat_thresholds:
        warnings.append("native peer heartbeat performance deltas are unavailable")
        for pattern, limit in heartbeat_thresholds:
            add_threshold_failure(
                failures,
                metric=f"native_peer_heartbeat.delta.performance_numeric_deltas.{pattern}",
                actual=None,
                limit=limit,
                message="native peer heartbeat performance deltas are unavailable",
            )

    return {
        "enabled": enabled,
        "ok": not failures,
        "thresholds": thresholds,
        "failures": failures,
        "warnings": warnings,
        "failure_count": len(failures),
    }


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

    collect_file_growth = (
        not args.skip_db
        or args.assert_idle
        or args.max_db_growth_bytes is not None
        or args.max_db_file_growth_bytes is not None
    )
    collect_sync_runs = bool(
        not args.skip_db or args.assert_idle or args.max_sync_run_delta
    )
    collect_db_metrics = bool(
        (args.assert_idle and not args.skip_db) or args.max_db_metric_delta
    )
    database_files_before = None
    if collect_file_growth:
        database_files_before = database_file_snapshot(root, args.db)
        report["database_file_snapshots"] = {"before_cpu": database_files_before}

    database_metrics_before = None
    if collect_db_metrics:
        database_metrics_before = database_metric_snapshot(
            root,
            args.db,
            max_tables=args.max_tables,
            max_dbstat_rows=args.max_dbstat_rows,
            max_chunk_rows=args.max_chunk_rows,
            retain_chunk_generations=args.retain_chunk_generations,
        )
        report["database_metric_snapshots"] = {"before_cpu": database_metrics_before}

    sync_runs_before = None
    if collect_sync_runs:
        sync_runs_before = sync_run_snapshot(root, args.db)
        report["sync_run_snapshots"] = {"before_cpu": sync_runs_before}

    service_performance_before = None
    if not args.skip_service_performance:
        service_performance_before = read_service_performance_status(root)

    heartbeat_before = None
    if not args.skip_heartbeat:
        heartbeat_before = read_native_peer_heartbeat(root)

    if args.skip_cpu:
        report["process"] = {"skipped": True}
    else:
        report["process"] = sample_process(
            args.pid,
            args.process_name,
            args.cpu_samples,
            args.cpu_interval,
        )

    if collect_file_growth:
        database_files_after = database_file_snapshot(root, args.db)
        report["database_file_snapshots"]["after_cpu"] = database_files_after
        report["database_file_growth"] = database_file_growth(
            database_files_before,
            database_files_after,
        )

    if collect_db_metrics:
        database_metrics_after = database_metric_snapshot(
            root,
            args.db,
            max_tables=args.max_tables,
            max_dbstat_rows=args.max_dbstat_rows,
            max_chunk_rows=args.max_chunk_rows,
            retain_chunk_generations=args.retain_chunk_generations,
        )
        report["database_metric_snapshots"]["after_cpu"] = database_metrics_after
        report["database_metric_delta"] = database_metric_delta(
            database_metrics_before,
            database_metrics_after,
        )

    if collect_sync_runs:
        sync_runs_after = sync_run_snapshot(root, args.db)
        report["sync_run_snapshots"]["after_cpu"] = sync_runs_after
        report["sync_run_delta"] = sync_run_snapshot_delta(sync_runs_before, sync_runs_after)

    if args.skip_service_performance:
        report["service_performance_status"] = {"skipped": True}
    else:
        service_performance_after = read_service_performance_status(root)
        report["service_performance_status"] = {
            "before": service_performance_before,
            "after": service_performance_after,
            "delta": service_performance_status_delta(
                service_performance_before,
                service_performance_after,
            ),
        }

    if args.skip_heartbeat:
        report["native_peer_heartbeat"] = {"skipped": True}
    else:
        heartbeat_after = read_native_peer_heartbeat(root)
        report["native_peer_heartbeat"] = {
            "before": heartbeat_before,
            "after": heartbeat_after,
            "delta": native_peer_heartbeat_delta(heartbeat_before, heartbeat_after),
        }

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

    report["assertions"] = evaluate_assertions(report, args)

    json.dump(report, sys.stdout, indent=2 if args.pretty else None, sort_keys=True)
    sys.stdout.write("\n")
    if report["assertions"].get("enabled") and not report["assertions"].get("ok"):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
