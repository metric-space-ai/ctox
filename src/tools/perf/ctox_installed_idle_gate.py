#!/usr/bin/env python3
"""Run the installed CTOX idle evidence gates and store artifacts.

This orchestrates the release path that matters for idle regressions:
`ctox upgrade --dev`, then passive CPU sampling without `ctox status`, then a
separate status-poll load gate, then process-mining liveness evidence.
"""

from __future__ import annotations

import argparse
import datetime as dt
import fnmatch
import hashlib
import json
import math
import os
from pathlib import Path
import shlex
import shutil
import statistics
import subprocess
import sys
import threading
import time
from typing import Any


SCHEMA = "ctox.installed_idle_gate.v1"
DEFAULT_PROCESS_NAME = "ctox-real"
DEFAULT_STATUS_DELTA_LIMITS: tuple[tuple[str, float], ...] = (
    ("channel_sync.*.activity_runs", 0.0),
    ("channel_sync.*.no_activity_runs", 0.0),
    ("channel_sync.*.error_runs", 0.0),
    ("ticket_sync.*.activity_runs", 0.0),
    ("ticket_sync.*.no_activity_runs", 0.0),
    ("ticket_sync.*.error_runs", 0.0),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run installed CTOX idle evidence gates and write artifacts.",
    )
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument(
        "--artifact-dir",
        type=Path,
        help="Directory for artifacts. Defaults to runtime/perf/installed-idle-<timestamp>.",
    )
    parser.add_argument(
        "--ctox-command",
        default="ctox",
        help="Installed ctox command. Parsed with shell-like quoting.",
    )
    parser.add_argument("--process-name", default=DEFAULT_PROCESS_NAME)
    parser.add_argument("--pid", type=int, help="Use a fixed ctox-real PID instead of pgrep.")
    parser.add_argument(
        "--skip-upgrade",
        action="store_true",
        help="Do not run `ctox upgrade --dev`. Use only for local dry checks.",
    )
    parser.add_argument(
        "--skip-gate-b",
        action="store_true",
        help="Skip the status-poll load gate.",
    )
    parser.add_argument(
        "--skip-gate-c",
        action="store_true",
        help="Skip process-mining spawn-liveness.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Write the planned commands without executing them.",
    )
    parser.add_argument("--post-upgrade-warmup-seconds", type=float, default=30.0)
    parser.add_argument("--gate-a-seconds", type=float, default=300.0)
    parser.add_argument("--gate-b-seconds", type=float, default=120.0)
    parser.add_argument("--cpu-interval", type=float, default=1.0)
    parser.add_argument("--status-interval", type=float, default=0.5)
    parser.add_argument("--status-timeout", type=float, default=10.0)
    parser.add_argument("--process-mining-timeout", type=float, default=300.0)
    parser.add_argument(
        "--release",
        action="store_true",
        help="Use release-duration passive idle sampling unless explicit seconds are passed.",
    )
    parser.add_argument(
        "--max-status-p95-ms",
        type=float,
        default=100.0,
        help="Gate B maximum status p95 latency.",
    )
    parser.add_argument(
        "--max-status-performance-delta",
        action="append",
        default=[],
        metavar="GLOB=VALUE",
        help="Maximum Gate B service performance delta. May be repeated.",
    )
    args = parser.parse_args()

    if args.cpu_interval <= 0:
        parser.error("--cpu-interval must be > 0")
    if args.status_interval <= 0:
        parser.error("--status-interval must be > 0")
    if args.status_timeout <= 0:
        parser.error("--status-timeout must be > 0")
    if args.gate_a_seconds <= 0:
        parser.error("--gate-a-seconds must be > 0")
    if args.gate_b_seconds <= 0:
        parser.error("--gate-b-seconds must be > 0")
    for raw in args.max_status_performance_delta:
        try:
            parse_metric_threshold(raw)
        except ValueError as err:
            parser.error(f"--max-status-performance-delta {err}")
    if args.release and args.gate_a_seconds == 300.0:
        args.gate_a_seconds = 600.0
    return args


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def safe_timestamp() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")


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


def command_from_text(text: str) -> list[str]:
    command = shlex.split(text)
    if not command:
        raise ValueError("empty command")
    return command


def parse_utc_timestamp(value: Any) -> dt.datetime | None:
    if not isinstance(value, str) or not value:
        return None
    try:
        return dt.datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(
            dt.timezone.utc
        )
    except ValueError:
        return None


def parse_ps_lstart(value: str | None) -> dt.datetime | None:
    if not value:
        return None
    text = " ".join(value.split())
    for fmt in ("%a %b %d %H:%M:%S %Y", "%a %b %e %H:%M:%S %Y"):
        try:
            parsed = dt.datetime.strptime(text, fmt)
            return parsed.astimezone(dt.timezone.utc)
        except ValueError:
            continue
    return None


def sample_count(seconds: float, interval: float) -> int:
    return max(1, int(math.ceil(seconds / interval)))


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def file_sha256(path: Path) -> str | None:
    try:
        digest = hashlib.sha256()
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
        return digest.hexdigest()
    except OSError:
        return None


def path_report(path: Path | None) -> dict[str, Any] | None:
    if path is None:
        return None
    report: dict[str, Any] = {
        "path": str(path),
        "exists": path.exists(),
        "is_symlink": path.is_symlink(),
    }
    try:
        report["resolved"] = str(path.resolve())
    except OSError as err:
        report["resolve_error"] = f"{type(err).__name__}: {err}"
    if path.exists() and path.is_file():
        report["sha256"] = file_sha256(path)
        try:
            stat = path.stat()
            report["size_bytes"] = stat.st_size
            report["mtime_ns"] = stat.st_mtime_ns
        except OSError as err:
            report["stat_error"] = f"{type(err).__name__}: {err}"
    return report


def read_json_object(path: Path) -> dict[str, Any] | None:
    try:
        parsed = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return parsed if isinstance(parsed, dict) else None


def run_capture(command: list[str], *, cwd: Path, timeout: float = 10.0) -> dict[str, Any]:
    result: dict[str, Any] = {"command": command}
    started = time.perf_counter()
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
        result.update(
            {
                "returncode": completed.returncode,
                "stdout": completed.stdout.strip(),
                "stderr": completed.stderr.strip(),
            }
        )
    except subprocess.TimeoutExpired as err:
        result.update({"returncode": None, "timeout": True, "error": str(err)})
    except OSError as err:
        result.update({"returncode": None, "error": f"{type(err).__name__}: {err}"})
    result["elapsed_seconds"] = time.perf_counter() - started
    return result


def run_command(
    command: list[str],
    *,
    cwd: Path,
    artifact_dir: Path,
    name: str,
    timeout: float | None = None,
    dry_run: bool = False,
    stdout_path: Path | None = None,
) -> dict[str, Any]:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    stdout_path = stdout_path or artifact_dir / f"{name}.stdout"
    stderr_path = artifact_dir / f"{name}.stderr"
    meta_path = artifact_dir / f"{name}.meta.json"
    result: dict[str, Any] = {
        "name": name,
        "command": command,
        "cwd": str(cwd),
        "started_at": utc_now(),
        "stdout_path": str(stdout_path),
        "stderr_path": str(stderr_path),
        "dry_run": dry_run,
    }
    if dry_run:
        result.update({"returncode": 0, "finished_at": utc_now(), "elapsed_seconds": 0.0})
        write_json(meta_path, result)
        return result

    started = time.perf_counter()
    try:
        with stdout_path.open("wb") as stdout_file, stderr_path.open("wb") as stderr_file:
            completed = subprocess.run(
                command,
                cwd=cwd,
                stdout=stdout_file,
                stderr=stderr_file,
                timeout=timeout,
                check=False,
            )
        result["returncode"] = completed.returncode
    except subprocess.TimeoutExpired as err:
        result.update({"returncode": None, "timeout": True, "error": str(err)})
    except OSError as err:
        result.update({"returncode": None, "error": f"{type(err).__name__}: {err}"})
    result["elapsed_seconds"] = time.perf_counter() - started
    result["finished_at"] = utc_now()
    write_json(meta_path, result)
    return result


def resolve_pid(pid: int | None, process_name: str, *, dry_run: bool = False) -> dict[str, Any]:
    if dry_run:
        return {
            "pid": pid if pid is not None else 0,
            "process_name": process_name,
            "source": "argument" if pid is not None else "dry_run",
            "candidates": [pid] if pid is not None else [],
            "extra_candidate_pids": [],
        }
    candidates: list[int] = []
    pgrep_report: dict[str, Any] = {
        "source": "pgrep -x",
        "process_name": process_name,
        "candidates": candidates,
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
        pgrep_report.update({"source": "pgrep", "error": str(err)})
    else:
        for raw in result.stdout.splitlines():
            try:
                candidates.append(int(raw.strip()))
            except ValueError:
                continue
        pgrep_report.update(
            {
                "returncode": result.returncode,
                "stderr": result.stderr.strip() or None,
            }
        )
    selected = pid if pid is not None else (max(candidates) if candidates else None)
    all_candidates = sorted(set(candidates + ([pid] if pid is not None else [])))
    return {
        "pid": selected,
        "process_name": process_name,
        "source": "argument" if pid is not None else pgrep_report.get("source"),
        "candidates": all_candidates,
        "pgrep": pgrep_report,
        "extra_candidate_pids": [
            candidate for candidate in all_candidates if selected is not None and candidate != selected
        ],
        "error": pgrep_report.get("error"),
        "returncode": pgrep_report.get("returncode"),
        "stderr": pgrep_report.get("stderr"),
    }


def ps_field(pid: int, field: str) -> str | None:
    try:
        result = subprocess.run(
            ["ps", "-p", str(pid), "-o", f"{field}="],
            text=True,
            capture_output=True,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if result.returncode != 0:
        return None
    value = result.stdout.strip()
    return value or None


def process_identity(pid: int, *, dry_run: bool) -> dict[str, Any]:
    if dry_run:
        return {"pid": pid, "dry_run": True}
    report: dict[str, Any] = {
        "pid": pid,
        "command": ps_field(pid, "command"),
        "comm": ps_field(pid, "comm"),
        "args": ps_field(pid, "args"),
        "lstart": ps_field(pid, "lstart"),
        "etime": ps_field(pid, "etime"),
    }
    parsed_start = parse_ps_lstart(report.get("lstart"))
    if parsed_start is not None:
        report["started_at_utc"] = parsed_start.isoformat().replace("+00:00", "Z")

    proc_exe = Path(f"/proc/{pid}/exe")
    executable_candidates: list[str] = []
    if proc_exe.exists():
        try:
            executable_candidates.append(str(proc_exe.resolve()))
        except OSError:
            executable_candidates.append(str(proc_exe))
    for key in ("comm", "command", "args"):
        value = report.get(key)
        if not isinstance(value, str) or not value.strip():
            continue
        first = shlex.split(value)[0] if value else ""
        if first:
            executable_candidates.append(first)

    existing_paths: list[Path] = []
    for candidate in executable_candidates:
        path = Path(candidate)
        if not path.is_absolute():
            resolved = shutil.which(candidate)
            if resolved:
                path = Path(resolved)
        if path.exists():
            existing_paths.append(path)
            break
    if existing_paths:
        report["executable"] = path_report(existing_paths[0])
    else:
        report["executable_candidates"] = executable_candidates
    return report


def infer_install_roots(ctox_command_path: Path | None, root: Path) -> list[Path]:
    candidates: list[Path] = []
    env_root = os.environ.get("CTOX_INSTALL_ROOT")
    if env_root:
        candidates.append(Path(env_root).expanduser())
    if ctox_command_path is not None:
        paths = [ctox_command_path]
        try:
            paths.append(ctox_command_path.resolve())
        except OSError:
            pass
        for path in paths:
            if path.name == "ctox" and path.parent.name == "bin":
                candidates.append(path.parent.parent)
    candidates.extend(
        [
            Path.home() / ".local" / "lib" / "ctox",
            root,
        ]
    )
    unique: list[Path] = []
    seen: set[str] = set()
    for candidate in candidates:
        try:
            resolved = candidate.expanduser().resolve()
        except OSError:
            resolved = candidate.expanduser()
        key = str(resolved)
        if key in seen:
            continue
        seen.add(key)
        unique.append(resolved)
    return unique


def select_install_root(candidates: list[Path]) -> Path | None:
    for candidate in candidates:
        if (candidate / "current").exists() or (candidate / "install_manifest.json").exists():
            return candidate
    for candidate in candidates:
        if (candidate / "bin" / "ctox").exists():
            return candidate
    return candidates[0] if candidates else None


def same_sha(left: dict[str, Any] | None, right: dict[str, Any] | None) -> bool | None:
    if not isinstance(left, dict) or not isinstance(right, dict):
        return None
    left_sha = left.get("sha256")
    right_sha = right.get("sha256")
    if not isinstance(left_sha, str) or not isinstance(right_sha, str):
        return None
    return left_sha == right_sha


def git_identity(root: Path, *, dry_run: bool) -> dict[str, Any]:
    commands = {
        "head": ["git", "rev-parse", "HEAD"],
        "branch": ["git", "branch", "--show-current"],
        "status_short": ["git", "status", "--short"],
    }
    if dry_run:
        return {
            key: {"dry_run": True, "command": command}
            for key, command in commands.items()
        }
    return {
        key: run_capture(command, cwd=root, timeout=10)
        for key, command in commands.items()
    }


def build_release_identity(
    *,
    ctox_command: list[str],
    root: Path,
    pid: int,
    upgrade_gate: dict[str, Any] | None,
    dry_run: bool,
) -> dict[str, Any]:
    command_lookup = shutil.which(ctox_command[0])
    command_path = Path(command_lookup).resolve() if command_lookup else None
    install_roots = infer_install_roots(command_path, root)
    install_root = select_install_root(install_roots)
    current_link = install_root / "current" if install_root is not None else None
    current_target = None
    if current_link is not None and (current_link.exists() or current_link.is_symlink()):
        try:
            current_target = current_link.resolve()
        except OSError:
            current_target = None

    install_manifest_path = (
        install_root / "install_manifest.json" if install_root is not None else None
    )
    install_manifest = (
        read_json_object(install_manifest_path)
        if install_manifest_path is not None and install_manifest_path.exists()
        else None
    )
    shared_real = install_root / "bin" / "ctox-real" if install_root is not None else None
    current_real = current_target / "bin" / "ctox-real" if current_target is not None else None

    process = process_identity(pid, dry_run=dry_run)
    version = (
        {"dry_run": True, "command": ctox_command + ["--version"]}
        if dry_run
        else run_capture(ctox_command + ["--version"], cwd=root, timeout=10)
    )

    identity: dict[str, Any] = {
        "schema": "ctox.installed_idle_gate.release_identity.v1",
        "generated_at": utc_now(),
        "dry_run": dry_run,
        "root": str(root),
        "source_git": git_identity(root, dry_run=dry_run),
        "ctox_command": ctox_command,
        "ctox_command_path": path_report(command_path),
        "ctox_version": version,
        "install_root_candidates": [str(path) for path in install_roots],
        "install_root": str(install_root) if install_root is not None else None,
        "install_manifest_path": str(install_manifest_path) if install_manifest_path else None,
        "install_manifest": install_manifest,
        "current_symlink": path_report(current_link),
        "current_target": path_report(current_target),
        "current_release_binary": path_report(current_real),
        "shared_launcher_binary": path_report(shared_real),
        "process": process,
        "upgrade_gate": {
            key: upgrade_gate.get(key)
            for key in ("started_at", "finished_at", "returncode", "dry_run")
            if isinstance(upgrade_gate, dict) and key in upgrade_gate
        },
    }
    identity["assertions"] = evaluate_release_identity(identity)
    return identity


def evaluate_release_identity(identity: dict[str, Any]) -> dict[str, Any]:
    failures: list[dict[str, Any]] = []
    warnings: list[str] = []
    if identity.get("dry_run"):
        return {"ok": True, "failures": failures, "warnings": warnings, "dry_run": True}

    install_root = identity.get("install_root")
    if not isinstance(install_root, str) or not install_root:
        failures.append(
            {
                "metric": "release_identity.install_root",
                "message": "install root was not resolved",
            }
        )

    current_target = identity.get("current_target")
    if not isinstance(current_target, dict) or not current_target.get("exists"):
        failures.append(
            {
                "metric": "release_identity.current",
                "message": "current release symlink target is missing",
            }
        )

    source_git = identity.get("source_git")
    git_head = source_git.get("head") if isinstance(source_git, dict) else None
    if (
        not isinstance(git_head, dict)
        or git_head.get("returncode") != 0
        or not isinstance(git_head.get("stdout"), str)
        or not git_head.get("stdout")
    ):
        failures.append(
            {
                "metric": "release_identity.source_git.head",
                "message": "source git commit could not be recorded",
            }
        )

    manifest = identity.get("install_manifest")
    if isinstance(manifest, dict):
        manifest_release = manifest.get("current_release")
        current_path = current_target.get("path") if isinstance(current_target, dict) else None
        if isinstance(manifest_release, str) and isinstance(current_path, str):
            if Path(current_path).name != manifest_release:
                failures.append(
                    {
                        "metric": "release_identity.current_release",
                        "message": "install_manifest current_release does not match current symlink target",
                        "manifest_current_release": manifest_release,
                        "current_target": current_path,
                    }
                )
    else:
        failures.append(
            {
                "metric": "release_identity.install_manifest",
                "message": "install manifest is missing or unreadable",
            }
        )

    current_real = identity.get("current_release_binary")
    shared_real = identity.get("shared_launcher_binary")
    process = identity.get("process")
    process_exe = process.get("executable") if isinstance(process, dict) else None
    current_shared_match = same_sha(current_real, shared_real)
    if current_shared_match is False:
        failures.append(
            {
                "metric": "release_identity.current_vs_shared_binary_sha256",
                "message": "current release ctox-real and shared launcher ctox-real differ",
            }
        )
    elif current_shared_match is None:
        failures.append(
            {
                "metric": "release_identity.current_vs_shared_binary_sha256",
                "message": "could not compare current and shared ctox-real hashes",
            }
        )

    process_shared_match = same_sha(process_exe, shared_real)
    process_current_match = same_sha(process_exe, current_real)
    if process_shared_match is False and process_current_match is False:
        failures.append(
            {
                "metric": "release_identity.process_binary_sha256",
                "message": "sampled ctox-real process binary does not match installed release binaries",
            }
        )
    elif process_shared_match is None and process_current_match is None:
        failures.append(
            {
                "metric": "release_identity.process_binary_sha256",
                "message": "could not hash sampled process executable",
            }
        )

    if isinstance(process, dict):
        command = process.get("command") or process.get("args") or process.get("comm")
        if isinstance(command, str) and "ctox-real" not in command:
            failures.append(
                {
                    "metric": "release_identity.process_command",
                    "message": "sampled process command does not look like ctox-real",
                    "command": command,
                }
            )
        process_started = parse_utc_timestamp(process.get("started_at_utc"))
        upgrade = identity.get("upgrade_gate")
        upgrade_started = (
            parse_utc_timestamp(upgrade.get("started_at"))
            if isinstance(upgrade, dict)
            else None
        )
        if process_started is None:
            warnings.append("process start time could not be parsed")
        elif upgrade_started is not None and process_started < upgrade_started:
            failures.append(
                {
                    "metric": "release_identity.process_start_time",
                    "message": "sampled process was already running before ctox upgrade --dev started",
                    "process_started_at": process.get("started_at_utc"),
                    "upgrade_started_at": (
                        upgrade.get("started_at") if isinstance(upgrade, dict) else None
                    ),
                }
            )
    else:
        failures.append(
            {
                "metric": "release_identity.process",
                "message": "process identity is missing",
            }
        )

    return {"ok": not failures, "failures": failures, "warnings": warnings}


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


def performance_delta(samples: list[dict[str, Any]]) -> dict[str, Any] | None:
    first = next(
        (sample.get("performance") for sample in samples if isinstance(sample.get("performance"), dict)),
        None,
    )
    last = next(
        (
            sample.get("performance")
            for sample in reversed(samples)
            if isinstance(sample.get("performance"), dict)
        ),
        None,
    )
    if not isinstance(first, dict) or not isinstance(last, dict):
        return None
    before = flatten_numeric_values(first)
    after = flatten_numeric_values(last)
    deltas = {}
    for key in sorted(set(before) & set(after)):
        delta = after[key] - before[key]
        if delta:
            deltas[key] = delta
    return {"performance_numeric_deltas": deltas}


def sample_status_poll_load(
    command: list[str],
    *,
    cwd: Path,
    artifact_dir: Path,
    seconds: float,
    interval: float,
    timeout: float,
    dry_run: bool,
) -> dict[str, Any]:
    samples_path = artifact_dir / "gate-b-status-samples.jsonl"
    if dry_run:
        summary = {
            "dry_run": True,
            "command": command,
            "seconds": seconds,
            "interval": interval,
            "sample_count": 0,
            "samples_path": str(samples_path),
        }
        write_json(artifact_dir / "gate-b-status-summary.json", summary)
        return summary

    samples: list[dict[str, Any]] = []
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        started = time.perf_counter()
        sample: dict[str, Any] = {"at": utc_now()}
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
            sample.update(
                {
                    "latency_ms": elapsed_ms,
                    "returncode": result.returncode,
                    "stdout_bytes": len(result.stdout.encode("utf-8", errors="replace")),
                    "stderr_tail": result.stderr[-1000:] or None,
                }
            )
            parsed = None
            if result.returncode == 0 and result.stdout.strip():
                try:
                    parsed = json.loads(result.stdout)
                except json.JSONDecodeError:
                    parsed = None
            sample["json_ok"] = parsed is not None
            if isinstance(parsed, dict) and isinstance(parsed.get("performance"), dict):
                sample["performance"] = parsed["performance"]
        except subprocess.TimeoutExpired as err:
            sample.update(
                {
                    "latency_ms": (time.perf_counter() - started) * 1000.0,
                    "timeout": True,
                    "error": str(err),
                }
            )
        except OSError as err:
            sample.update(
                {
                    "latency_ms": (time.perf_counter() - started) * 1000.0,
                    "error": f"{type(err).__name__}: {err}",
                }
            )
        samples.append(sample)
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        time.sleep(min(interval, remaining))

    with samples_path.open("w", encoding="utf-8") as handle:
        for sample in samples:
            handle.write(json.dumps(sample, sort_keys=True) + "\n")

    latencies = [
        float(sample["latency_ms"])
        for sample in samples
        if isinstance(sample.get("latency_ms"), (int, float)) and not sample.get("timeout")
    ]
    summary = {
        "dry_run": False,
        "command": command,
        "seconds": seconds,
        "interval": interval,
        "sample_count": len(samples),
        "samples_path": str(samples_path),
        "ok": bool(latencies) and all(sample.get("returncode") == 0 for sample in samples),
        "latency_ms_avg": statistics.fmean(latencies) if latencies else None,
        "latency_ms_min": min(latencies) if latencies else None,
        "latency_ms_max": max(latencies) if latencies else None,
        "latency_ms_p95": percentile(latencies, 0.95),
        "performance_delta": performance_delta(samples),
    }
    write_json(artifact_dir / "gate-b-status-summary.json", summary)
    return summary


def evaluate_status_poll_summary(
    summary: dict[str, Any],
    *,
    max_status_p95_ms: float,
    delta_limits: list[tuple[str, float]],
) -> dict[str, Any]:
    failures: list[dict[str, Any]] = []
    warnings: list[str] = []
    p95 = summary.get("latency_ms_p95")
    if not isinstance(p95, (int, float)) or isinstance(p95, bool):
        failures.append(
            {
                "metric": "status_poll.latency_ms_p95",
                "actual": p95,
                "limit": max_status_p95_ms,
                "message": "status p95 latency is unavailable",
            }
        )
    elif p95 > max_status_p95_ms:
        failures.append(
            {
                "metric": "status_poll.latency_ms_p95",
                "actual": p95,
                "limit": max_status_p95_ms,
                "message": "status p95 latency exceeded configured limit",
            }
        )

    performance = summary.get("performance_delta")
    deltas = (
        performance.get("performance_numeric_deltas")
        if isinstance(performance, dict)
        else None
    )
    sample_count = summary.get("sample_count")
    expected_status_delta = (
        max(0, int(sample_count) - 1)
        if isinstance(sample_count, int) and not isinstance(sample_count, bool)
        else None
    )
    if isinstance(deltas, dict):
        total_status_delta = deltas.get("status_requests.total_requests")
        if expected_status_delta is not None:
            if not isinstance(total_status_delta, (int, float)) or isinstance(
                total_status_delta, bool
            ):
                failures.append(
                    {
                        "metric": "status_poll.performance_delta.status_requests.total_requests",
                        "actual": total_status_delta,
                        "limit": expected_status_delta,
                        "message": "status request delta is unavailable",
                    }
                )
            elif total_status_delta < expected_status_delta:
                failures.append(
                    {
                        "metric": "status_poll.performance_delta.status_requests.total_requests",
                        "actual": total_status_delta,
                        "limit": expected_status_delta,
                        "message": "status request delta did not cover the status-poll load",
                    }
                )
        all_numbers = flatten_numeric_values(
            next(
                (
                    sample.get("performance")
                    for sample in read_status_samples(Path(summary["samples_path"]))
                    if isinstance(sample.get("performance"), dict)
                ),
                {},
            )
        )
        for pattern, limit in delta_limits:
            candidate_keys = {key for key in all_numbers if fnmatch.fnmatchcase(key, pattern)}
            candidate_keys.update(key for key in deltas if fnmatch.fnmatchcase(key, pattern))
            if not candidate_keys:
                warnings.append(f"status performance delta pattern matched no metrics: {pattern}")
                continue
            for key in sorted(candidate_keys):
                value = deltas.get(key, 0.0)
                if isinstance(value, (int, float)) and not isinstance(value, bool) and value > limit:
                    failures.append(
                        {
                            "metric": f"status_poll.performance_delta.{key}",
                            "actual": value,
                            "limit": limit,
                            "message": f"status performance delta {key} exceeded configured limit",
                        }
                    )
    else:
        warnings.append("status performance deltas are unavailable")
        if expected_status_delta is not None:
            failures.append(
                {
                    "metric": "status_poll.performance_delta.status_requests.total_requests",
                    "actual": None,
                    "limit": expected_status_delta,
                    "message": "status request delta is unavailable",
                }
            )
    return {
        "ok": not failures,
        "failures": failures,
        "warnings": warnings,
    }


def read_status_samples(path: Path) -> list[dict[str, Any]]:
    samples = []
    try:
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            try:
                parsed = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(parsed, dict):
                samples.append(parsed)
    except OSError:
        pass
    return samples


def main() -> int:
    args = parse_args()
    root = args.root.expanduser().resolve()
    artifact_dir = (
        args.artifact_dir.expanduser().resolve()
        if args.artifact_dir
        else root / "runtime" / "perf" / f"installed-idle-{safe_timestamp()}"
    )
    artifact_dir.mkdir(parents=True, exist_ok=True)

    try:
        ctox_command = command_from_text(args.ctox_command)
    except ValueError as err:
        raise SystemExit(f"invalid --ctox-command: {err}") from err

    probe = Path(__file__).with_name("ctox_perf_probe.py")
    status_command = ctox_command + ["status", "--json"]
    summary: dict[str, Any] = {
        "schema": SCHEMA,
        "generated_at": utc_now(),
        "root": str(root),
        "artifact_dir": str(artifact_dir),
        "dry_run": args.dry_run,
        "ctox_command": ctox_command,
        "gates": {},
        "host": {"platform": sys.platform, "cwd": os.getcwd()},
    }

    manifest = {
        "schema": SCHEMA,
        "root": str(root),
        "artifact_dir": str(artifact_dir),
        "generated_at": summary["generated_at"],
        "dry_run": args.dry_run,
        "planned": {
            "upgrade": None if args.skip_upgrade else ctox_command + ["upgrade", "--dev"],
            "gate_a_seconds": args.gate_a_seconds,
            "gate_b_seconds": None if args.skip_gate_b else args.gate_b_seconds,
            "gate_c": None if args.skip_gate_c else ctox_command + ["process-mining", "spawn-liveness"],
        },
    }
    write_json(artifact_dir / "manifest.json", manifest)

    if not args.skip_upgrade:
        summary["gates"]["upgrade"] = run_command(
            ctox_command + ["upgrade", "--dev"],
            cwd=root,
            artifact_dir=artifact_dir,
            name="00-upgrade-dev",
            dry_run=args.dry_run,
        )
        if summary["gates"]["upgrade"].get("returncode") not in (0, None):
            write_json(artifact_dir / "summary.json", summary)
            return 1
        if not args.dry_run and args.post_upgrade_warmup_seconds > 0:
            time.sleep(args.post_upgrade_warmup_seconds)

    upgrade_gate = summary["gates"].get("upgrade")
    pid_resolution = resolve_pid(args.pid, args.process_name, dry_run=args.dry_run)
    summary["pid_resolution"] = pid_resolution
    write_json(artifact_dir / "pid-resolution.json", pid_resolution)
    pid = pid_resolution.get("pid")
    if not isinstance(pid, int):
        summary["ok"] = False
        summary["error"] = "ctox-real PID was not resolved"
        write_json(artifact_dir / "summary.json", summary)
        return 1
    extra_candidate_pids = pid_resolution.get("extra_candidate_pids")
    if not args.dry_run and isinstance(extra_candidate_pids, list) and extra_candidate_pids:
        summary["ok"] = False
        summary["failures"] = [
            {
                "gate": "process_inventory",
                "message": "extra ctox-real candidate processes were present",
                "selected_pid": pid,
                "extra_candidate_pids": extra_candidate_pids,
            }
        ]
        write_json(artifact_dir / "summary.json", summary)
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 1

    release_identity = build_release_identity(
        ctox_command=ctox_command,
        root=root,
        pid=pid,
        upgrade_gate=upgrade_gate if isinstance(upgrade_gate, dict) else None,
        dry_run=args.dry_run,
    )
    summary["release_identity"] = {
        "path": str(artifact_dir / "release-identity.json"),
        "assertions": release_identity.get("assertions"),
    }
    write_json(artifact_dir / "release-identity.json", release_identity)
    release_assertions = release_identity.get("assertions")
    if isinstance(release_assertions, dict) and not release_assertions.get("ok"):
        summary["ok"] = False
        summary["failures"] = [
            {
                "gate": "release_identity",
                "assertions": release_assertions.get("failures"),
            }
        ]
        write_json(artifact_dir / "summary.json", summary)
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 1

    gate_a_command = [
        sys.executable,
        str(probe),
        "--root",
        str(root),
        "--pid",
        str(pid),
        "--assert-idle",
        "--skip-status",
        "--cpu-samples",
        str(sample_count(args.gate_a_seconds, args.cpu_interval)),
        "--cpu-interval",
        str(args.cpu_interval),
        "--pretty",
    ]
    summary["gates"]["gate_a_passive_idle"] = run_command(
        gate_a_command,
        cwd=root,
        artifact_dir=artifact_dir,
        name="10-gate-a-passive-idle",
        dry_run=args.dry_run,
        stdout_path=artifact_dir / "10-gate-a-passive-idle.json",
    )

    if not args.skip_gate_b:
        gate_b_probe_command = [
            sys.executable,
            str(probe),
            "--root",
            str(root),
            "--pid",
            str(pid),
            "--assert-idle",
            "--skip-status",
            "--skip-service-performance",
            "--cpu-samples",
            str(sample_count(args.gate_b_seconds, args.cpu_interval)),
            "--cpu-interval",
            str(args.cpu_interval),
            "--pretty",
        ]
        status_summary_box: dict[str, Any] = {}

        def poll_status() -> None:
            status_summary_box["summary"] = sample_status_poll_load(
                status_command,
                cwd=root,
                artifact_dir=artifact_dir,
                seconds=args.gate_b_seconds,
                interval=args.status_interval,
                timeout=args.status_timeout,
                dry_run=args.dry_run,
            )

        poller = threading.Thread(target=poll_status, daemon=True)
        poller.start()
        gate_b_probe = run_command(
            gate_b_probe_command,
            cwd=root,
            artifact_dir=artifact_dir,
            name="20-gate-b-status-load-probe",
            dry_run=args.dry_run,
            stdout_path=artifact_dir / "20-gate-b-status-load-probe.json",
        )
        poller.join(timeout=max(args.gate_b_seconds + args.status_timeout + 5.0, 10.0))
        status_summary = status_summary_box.get("summary", {})
        delta_limits = list(DEFAULT_STATUS_DELTA_LIMITS)
        delta_limits.extend(
            parse_metric_threshold(raw) for raw in args.max_status_performance_delta
        )
        status_assertions = (
            {"ok": True, "failures": [], "warnings": [], "dry_run": True}
            if args.dry_run
            else evaluate_status_poll_summary(
                status_summary,
                max_status_p95_ms=args.max_status_p95_ms,
                delta_limits=delta_limits,
            )
        )
        write_json(artifact_dir / "gate-b-status-assertions.json", status_assertions)
        summary["gates"]["gate_b_status_load"] = {
            "probe": gate_b_probe,
            "status_summary": status_summary,
            "status_assertions": status_assertions,
        }

    if not args.skip_gate_c:
        summary["gates"]["gate_c_process_mining"] = run_command(
            ctox_command + ["process-mining", "spawn-liveness"],
            cwd=root,
            artifact_dir=artifact_dir,
            name="30-gate-c-process-mining-spawn-liveness",
            timeout=args.process_mining_timeout,
            dry_run=args.dry_run,
        )

    gate_failures = []
    for name, gate in summary["gates"].items():
        if isinstance(gate, dict) and "returncode" in gate and gate.get("returncode") != 0:
            gate_failures.append({"gate": name, "returncode": gate.get("returncode")})
        if name == "gate_b_status_load" and isinstance(gate, dict):
            probe_result = gate.get("probe")
            assertions = gate.get("status_assertions")
            if isinstance(probe_result, dict) and probe_result.get("returncode") != 0:
                gate_failures.append({"gate": name, "returncode": probe_result.get("returncode")})
            if isinstance(assertions, dict) and not assertions.get("ok"):
                gate_failures.append({"gate": name, "assertions": assertions.get("failures")})

    summary["ok"] = not gate_failures
    summary["failures"] = gate_failures
    write_json(artifact_dir / "summary.json", summary)
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
