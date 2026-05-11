#!/usr/bin/env python3
"""Controller for CTOX Terminal-Bench queue runs.

The controller is intentionally external to CTOX. It reads the queue manifests,
matches each task to the CTOX routing DB, validates completed workspaces through
the official Terminal-Bench runner, and writes a stable ledger.

It never counts CTOX `handled` as success. Only `tb run` replay results can
produce `passed` or `model_failed`.
"""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


DEFAULT_DB = Path("/home/metricspace/ctox/runtime/ctox.sqlite3")
DEFAULT_TASK_ROOT = Path("/home/metricspace/.cache/terminal-bench/terminal-bench-core/0.1.1")
DEFAULT_OUTPUT_DIR = Path("/home/metricspace/ctox/runtime/terminal-bench-controller")
DEFAULT_TB_BIN = Path("/home/metricspace/.local/bin/tb")
DEFAULT_REPLAY_AGENT_DIR = Path("/home/metricspace/ctox/runtime")
DEFAULT_MIN_FREE_GB = 25.0


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def safe_slug(value: str) -> str:
    value = re.sub(r"[^A-Za-z0-9_.-]+", "-", value.strip())
    return value.strip("-")[:180] or "task"


def short_validation_id(index: int, task: str, workspace: Path) -> str:
    digest = hashlib.sha1(str(workspace).encode("utf-8")).hexdigest()[:10]
    task_slug = safe_slug(task)[:48]
    return f"{index:03d}-{task_slug}-{digest}"


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_no, raw_line in enumerate(handle, start=1):
            line = raw_line.strip()
            if not line:
                continue
            payload = json.loads(line)
            if not isinstance(payload, dict) or "workspace" not in payload:
                continue
            payload["_manifest"] = str(path)
            payload["_manifest_line"] = line_no
            records.append(payload)
    return records


def load_manifest_records(paths: list[Path]) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for path in paths:
        records.extend(read_jsonl(path))
    seen: set[str] = set()
    duplicates: list[str] = []
    for record in records:
        workspace = str(record["workspace"])
        if workspace in seen:
            duplicates.append(workspace)
        seen.add(workspace)
    if duplicates:
        raise SystemExit(f"duplicate workspace entries in manifests: {duplicates[:5]}")
    return records


def connect_db(path: Path) -> sqlite3.Connection:
    conn = sqlite3.connect(path)
    conn.row_factory = sqlite3.Row
    return conn


def route_for_workspace(conn: sqlite3.Connection, workspace: str) -> dict[str, Any]:
    rows = conn.execute(
        """
        SELECT
            m.message_key,
            m.subject,
            m.thread_key,
            m.preview,
            r.route_status,
            r.lease_owner,
            r.leased_at,
            r.acked_at,
            r.last_error,
            r.updated_at
        FROM communication_messages m
        JOIN communication_routing_state r ON r.message_key = m.message_key
        WHERE m.channel = ?
          AND m.direction = ?
          AND json_extract(m.metadata_json, ?) = ?
        ORDER BY r.updated_at DESC, m.message_key DESC
        """,
        ("queue", "inbound", "$.workspace_root", workspace),
    ).fetchall()
    if not rows:
        return {
            "route_status": "not_found",
            "route_rows": 0,
            "message_key": "",
            "subject": "",
            "thread_key": "",
            "lease_owner": "",
            "leased_at": "",
            "acked_at": "",
            "last_error": "",
            "updated_at": "",
        }
    row = dict(rows[0])
    row["route_rows"] = len(rows)
    row["lease_owner"] = row.get("lease_owner") or ""
    row["leased_at"] = row.get("leased_at") or ""
    row["acked_at"] = row.get("acked_at") or ""
    row["last_error"] = row.get("last_error") or ""
    return row


def free_gb(path: Path) -> float:
    usage = shutil.disk_usage(path)
    return usage.free / (1024**3)


def docker_prune() -> None:
    subprocess.run(["docker", "builder", "prune", "-af"], text=True, capture_output=True, check=False)


def docker_cleanup_for_task(*, run_id: str, task: str, workspace: Path) -> None:
    """Remove stale Docker resources that belong to this replayed TB task.

    Terminal-Bench tasks may expose fixed host ports or use compose-managed
    names. A previous CTOX worker run can leave those containers behind and make
    the official validation fail before the replay agent starts. Limit cleanup to
    containers/networks that carry this run/workspace/task identity.
    """
    tokens = {
        safe_slug(run_id).lower(),
        safe_slug(task).lower(),
        safe_slug(workspace.name).lower(),
    }
    tokens = {token for token in tokens if len(token) >= 6}

    def should_remove(name: str) -> bool:
        lowered = name.lower()
        if safe_slug(workspace.name).lower() in lowered:
            return True
        if safe_slug(run_id).lower() in lowered and safe_slug(task).lower() in lowered:
            return True
        return False

    ps = subprocess.run(
        ["docker", "ps", "-a", "--format", "{{.ID}}	{{.Names}}"],
        text=True,
        capture_output=True,
        check=False,
    )
    ids: list[str] = []
    for line in ps.stdout.splitlines():
        container_id, _, name = line.partition("	")
        if container_id and should_remove(name):
            ids.append(container_id)
    if ids:
        subprocess.run(["docker", "rm", "-f", *ids], text=True, capture_output=True, check=False)

    networks = subprocess.run(
        ["docker", "network", "ls", "--format", "{{.Name}}"],
        text=True,
        capture_output=True,
        check=False,
    )
    remove_networks = [
        name
        for name in networks.stdout.splitlines()
        if name not in {"bridge", "host", "none"} and should_remove(name)
    ]
    for name in remove_networks:
        subprocess.run(["docker", "network", "rm", name], text=True, capture_output=True, check=False)


def read_validation_diagnostics(validation_dir: Path, *, max_bytes: int = 512_000) -> str:
    parts: list[str] = []
    patterns = [
        "**/agent.log",
        "**/post-agent.txt",
        "**/run.log",
        "**/commands.txt",
        "**/ctox_replay_overlay_audit.json",
        "**/.ctox_replay_overlay_audit.json",
    ]
    for pattern in patterns:
        for path in sorted(validation_dir.glob(pattern)):
            if not path.is_file():
                continue
            try:
                data = path.read_bytes()
            except OSError:
                continue
            if len(data) > max_bytes:
                data = data[-max_bytes:]
            parts.append(f"\n--- {path} ---\n")
            parts.append(data.decode("utf-8", errors="replace"))
    return "".join(parts)


def cached_validation_needs_replay(cached: dict[str, Any], validation_dir: Path) -> bool:
    validation_status = cached.get("validation_status")
    if validation_status == "infra_failed" and cached.get("is_resolved") is True:
        return True
    if cached.get("failure_mode") in {"unknown_agent_error", "missing_results_json"}:
        return True
    combined = ""
    for key in ("stdout_path", "stderr_path"):
        raw = cached.get(key) or ""
        if raw:
            path = Path(raw)
            if path.is_file():
                combined += "\n" + path.read_text(encoding="utf-8", errors="replace")
    combined += read_validation_diagnostics(validation_dir)
    if classify_infra_text(combined):
        return True
    return validation_status == "validator_error"


def parse_tb_result(path: Path) -> dict[str, Any]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    results = payload.get("results") or []
    if not results:
        return {
            "validation_status": "validator_error",
            "is_resolved": None,
            "failure_mode": "missing_result",
            "parser_results": None,
            "accuracy": payload.get("accuracy"),
        }
    result = results[0]
    is_resolved = result.get("is_resolved")
    failure_mode = result.get("failure_mode")
    parser_results = result.get("parser_results")
    if is_resolved is True:
        status = "passed"
    elif is_resolved is False:
        status = "model_failed"
    elif failure_mode == "parse_error" and payload.get("n_unresolved", 0) > 0:
        # Terminal-Bench produced a result artifact and marked the trial unresolved.
        # Keep real missing-result/runner failures as validator_error, but do not
        # let model-induced test collection/parse failures pollute infra counts.
        status = "model_failed"
    elif failure_mode == "unknown_agent_error" and not result.get("trial_started_at"):
        # The official harness never reached the trial/agent/test phase. This is
        # a replay infrastructure failure, not model benchmark evidence.
        status = "infra_failed"
    elif failure_mode == "unknown_agent_error" and not result.get("agent_started_at"):
        status = "infra_failed"
    else:
        status = "validator_error"
    return {
        "validation_status": status,
        "is_resolved": is_resolved,
        "failure_mode": failure_mode,
        "parser_results": parser_results,
        "accuracy": payload.get("accuracy"),
        "trial_started_at": result.get("trial_started_at"),
        "trial_ended_at": result.get("trial_ended_at"),
    }


def classify_infra_text(text: str) -> str:
    lower = text.lower()
    replay_infra_markers = [
        "cp: cannot overwrite",
        "cannot overwrite directory",
        "runtimeerror: missing replay source",
        "runtimeerror: missing app target",
    ]
    if any(marker in lower for marker in replay_infra_markers):
        return "infra_failed"
    if "ctox_replay_overlay_failed" in lower and "traceback (most recent call last)" in lower:
        return "infra_failed"
    if "no space left on device" in lower:
        return "infra_failed"
    if "error creating docker client" in lower or "dockerexception" in lower:
        return "infra_failed"
    if "connection aborted" in lower and "docker" in lower:
        return "infra_failed"
    docker_compose_failed = (
        ("docker compose" in lower or "docker, compose" in lower or "docker", "compose" in lower)
        and "returned non-zero exit status" in lower
    )
    if docker_compose_failed:
        return "infra_failed"
    docker_infra_markers = [
        "failed to set up container networking",
        "port is already allocated",
        "bind for 0.0.0.0",
        "is already in use by container",
        "conflict. the container name",
        "cannot connect to the docker daemon",
        "network not found",
    ]
    if any(marker in lower for marker in docker_infra_markers):
        return "infra_failed"
    return ""


def existing_validation(validation_dir: Path) -> dict[str, Any] | None:
    meta_path = validation_dir / "validation.json"
    if meta_path.is_file():
        return json.loads(meta_path.read_text(encoding="utf-8"))
    return None


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False, sort_keys=True) + "\n", encoding="utf-8")


def run_replay_validation(
    *,
    record: dict[str, Any],
    output_dir: Path,
    task_root: Path,
    tb_bin: Path,
    replay_agent_dir: Path,
    agent_timeout_sec: int,
    test_timeout_sec: int,
    min_free_gb: float,
    prune_before: bool,
) -> dict[str, Any]:
    task = str(record["task"])
    workspace = Path(record["workspace"])
    index = int(record.get("index") or 0)
    slug = safe_slug(f"{index:03d}-{task}-{workspace.name}")
    short_id = short_validation_id(index, task, workspace)
    validation_dir = output_dir / "validations" / slug
    cached = existing_validation(validation_dir)
    if cached:
        needs_replay = cached_validation_needs_replay(cached, validation_dir)
        if not needs_replay:
            return cached
        backup_stamp = utc_now().replace(":", "").replace("-", "")
        backup = validation_dir.with_name(f"{validation_dir.name}.stale-{backup_stamp}")
        if not backup.exists():
            validation_dir.rename(backup)

    validation_dir.mkdir(parents=True, exist_ok=True)
    docker_cleanup_for_task(run_id=record.get("run_id") or "", task=task, workspace=workspace)
    if prune_before:
        docker_prune()
    free = free_gb(Path("/"))
    if free < min_free_gb:
        result = {
            "validation_status": "infra_blocked",
            "error": f"free disk {free:.1f}GB below threshold {min_free_gb:.1f}GB",
            "free_gb": round(free, 2),
            "tb_results_path": "",
            "stdout_path": "",
            "stderr_path": "",
            "validated_at": utc_now(),
        }
        write_json(validation_dir / "validation.json", result)
        return result

    run_id = f"replay-{short_id}"
    stdout_path = validation_dir / "tb.stdout.txt"
    stderr_path = validation_dir / "tb.stderr.txt"
    command = [
        str(tb_bin),
        "run",
        "--dataset-path",
        str(task_root),
        "--task-id",
        task,
        "--agent-import-path",
        "tbench_workspace_replay_agent:WorkspaceReplayAgent",
        "--agent-kwarg",
        f"workspace_root={workspace}",
        "--output-path",
        str(validation_dir),
        "--run-id",
        run_id,
        "--n-concurrent",
        "1",
        "--global-agent-timeout-sec",
        str(agent_timeout_sec),
        "--global-test-timeout-sec",
        str(test_timeout_sec),
        "--no-upload-results",
        "--cleanup",
    ]
    env = os.environ.copy()
    env["PYTHONPATH"] = f"{replay_agent_dir}:{env.get('PYTHONPATH', '')}".rstrip(":")
    started = time.time()
    validation_cwd = output_dir
    validation_cwd.mkdir(parents=True, exist_ok=True)
    completed = subprocess.run(
        command,
        text=True,
        capture_output=True,
        env=env,
        cwd=str(validation_cwd),
        check=False,
    )
    stdout_path.write_text(completed.stdout, encoding="utf-8", errors="replace")
    stderr_path.write_text(completed.stderr, encoding="utf-8", errors="replace")
    result_path = validation_dir / run_id / "results.json"
    combined = completed.stdout + "\n" + completed.stderr + read_validation_diagnostics(validation_dir)
    infra_status = classify_infra_text(combined)
    if result_path.is_file() and not infra_status:
        parsed = parse_tb_result(result_path)
    elif result_path.is_file():
        parsed = parse_tb_result(result_path)
        parsed["validation_status"] = infra_status
    elif completed.returncode < 0:
        parsed = {
            "validation_status": "infra_blocked",
            "is_resolved": None,
            "failure_mode": "validator_interrupted",
            "parser_results": None,
            "accuracy": None,
        }
    else:
        parsed = {
            "validation_status": infra_status or "validator_error",
            "is_resolved": None,
            "failure_mode": "missing_results_json",
            "parser_results": None,
            "accuracy": None,
        }
    result = {
        **parsed,
        "command": command,
        "exit_code": completed.returncode,
        "duration_sec": round(time.time() - started, 3),
        "free_gb_before": round(free, 2),
        "cwd": str(validation_cwd),
        "tb_results_path": str(result_path) if result_path.exists() else "",
        "stdout_path": str(stdout_path),
        "stderr_path": str(stderr_path),
        "validated_at": utc_now(),
    }
    write_json(validation_dir / "validation.json", result)
    return result


def validation_for_not_ready(route_status: str) -> dict[str, Any]:
    status_map = {
        "pending": "not_started",
        "leased": "in_progress",
        "review_rework": "needs_rework",
        "blocked": "blocked",
        "not_found": "not_found",
    }
    return {
        "validation_status": status_map.get(route_status, "not_ready"),
        "is_resolved": None,
        "failure_mode": "",
        "parser_results": None,
        "tb_results_path": "",
        "validated_at": "",
    }


def build_ledger(args: argparse.Namespace) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    records = load_manifest_records(args.manifest)
    conn = connect_db(args.db)
    ledger: list[dict[str, Any]] = []
    for record in records:
        route = route_for_workspace(conn, str(record["workspace"]))
        if route["route_status"] == "handled" and args.validate_handled:
            validation = run_replay_validation(
                record=record,
                output_dir=args.output_dir,
                task_root=args.task_root,
                tb_bin=args.tb_bin,
                replay_agent_dir=args.replay_agent_dir,
                agent_timeout_sec=args.agent_timeout_sec,
                test_timeout_sec=args.test_timeout_sec,
                min_free_gb=args.min_free_gb,
                prune_before=args.prune_before_validate,
            )
        else:
            validation = validation_for_not_ready(route["route_status"])
        ledger.append(
            {
                "run_id": args.run_id,
                "updated_at": utc_now(),
                "manifest": record.get("_manifest", ""),
                "manifest_line": record.get("_manifest_line", 0),
                "index": record.get("index"),
                "task": record.get("task"),
                "difficulty": record.get("difficulty"),
                "workspace": record.get("workspace"),
                "task_dir": record.get("task_dir"),
                "thread_key": record.get("thread_key"),
                "title": record.get("title"),
                "routing": route,
                "validation": validation,
            }
        )

    route_counts: dict[str, int] = {}
    validation_counts: dict[str, int] = {}
    for row in ledger:
        route_status = row["routing"]["route_status"]
        validation_status = row["validation"]["validation_status"]
        route_counts[route_status] = route_counts.get(route_status, 0) + 1
        validation_counts[validation_status] = validation_counts.get(validation_status, 0) + 1
    summary = {
        "run_id": args.run_id,
        "updated_at": utc_now(),
        "tasks": len(ledger),
        "route_counts": dict(sorted(route_counts.items())),
        "validation_counts": dict(sorted(validation_counts.items())),
        "passed": validation_counts.get("passed", 0),
        "model_failed": validation_counts.get("model_failed", 0),
        "infra_failed": validation_counts.get("infra_failed", 0),
        "infra_blocked": validation_counts.get("infra_blocked", 0),
        "validator_error": validation_counts.get("validator_error", 0),
        "in_progress": validation_counts.get("in_progress", 0),
        "not_started": validation_counts.get("not_started", 0),
        "blocked": validation_counts.get("blocked", 0),
        "needs_rework": validation_counts.get("needs_rework", 0),
        "ledger_jsonl": str(args.output_dir / "bench-ledger.jsonl"),
        "summary_json": str(args.output_dir / "summary.json"),
    }
    return ledger, summary


def write_outputs(output_dir: Path, ledger: list[dict[str, Any]], summary: dict[str, Any]) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    ledger_path = output_dir / "bench-ledger.jsonl"
    with ledger_path.open("w", encoding="utf-8") as handle:
        for row in ledger:
            handle.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")
    write_json(output_dir / "summary.json", summary)
    print(json.dumps(summary, indent=2, ensure_ascii=False, sort_keys=True))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, action="append", required=True)
    parser.add_argument("--run-id", default="tbq-20260507T164-controller")
    parser.add_argument("--db", type=Path, default=DEFAULT_DB)
    parser.add_argument("--task-root", type=Path, default=DEFAULT_TASK_ROOT)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR)
    parser.add_argument("--tb-bin", type=Path, default=DEFAULT_TB_BIN)
    parser.add_argument("--replay-agent-dir", type=Path, default=DEFAULT_REPLAY_AGENT_DIR)
    parser.add_argument("--validate-handled", action="store_true")
    parser.add_argument("--prune-before-validate", action="store_true")
    parser.add_argument("--min-free-gb", type=float, default=DEFAULT_MIN_FREE_GB)
    parser.add_argument("--agent-timeout-sec", type=int, default=120)
    parser.add_argument("--test-timeout-sec", type=int, default=900)
    parser.add_argument("--loop", action="store_true")
    parser.add_argument("--interval-sec", type=int, default=120)
    parser.add_argument("--max-iterations", type=int, default=0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    lock_path = args.output_dir / "controller.lock"
    iteration = 0
    with lock_path.open("w", encoding="utf-8") as lock_file:
        while True:
            iteration += 1
            fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX)
            try:
                ledger, summary = build_ledger(args)
                summary["iteration"] = iteration
                write_outputs(args.output_dir, ledger, summary)
            finally:
                fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)
            if not args.loop:
                return 0
            if args.max_iterations and iteration >= args.max_iterations:
                return 0
            time.sleep(args.interval_sec)


if __name__ == "__main__":
    raise SystemExit(main())
