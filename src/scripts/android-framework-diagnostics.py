#!/usr/bin/env python3
"""Bounded host-side diagnostics for an Android emulator smoke test.

Raw adb output is never inherited or persisted.  It is read in bounded chunks
and reduced to an allowlisted schema.  Cleanup addresses only the PID and Linux
start identity recorded by the start command.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import math
import os
import re
import select
import signal
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Any, Callable, Iterable

RESULT_SCHEMA = "android-framework-diagnostics-v2"
OWNERSHIP_SCHEMA = "android-framework-diagnostics-ownership-v2"
DEFAULT_SERIAL = "emulator-5554"
DEFAULT_INTERVAL_SECONDS = 5.0
DEFAULT_LIFETIME_SECONDS = 1500.0
DEFAULT_COMMAND_TIMEOUT_SECONDS = 3.0
MAX_INTERVAL_SECONDS = 60.0
MAX_LIFETIME_SECONDS = 1500.0
MAX_COMMAND_TIMEOUT_SECONDS = 3.0
MAX_OUTPUT_BYTES = 256 * 1024
MAX_LOG_BYTES = 128 * 1024
LOG_READ_CHUNK_BYTES = 4096
STOP_GRACE_SECONDS = 3.0
KILL_GRACE_SECONDS = 3.0
MAX_PID = 4194304
THREADTIME_LOG = re.compile(
    r"^\s*(\d{2}-\d{2})\s+(\d{2}:\d{2}:\d{2}\.\d{3})\s+"
    r"(\d{1,10})\s+(\d{1,10})\s+([VDIWEF])\s+([\w.-]+)\s*:\s"
)
TIMESTAMP = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z$"
)
CRASH_CATEGORIES = (
    "system_server_fatal",
    "system_server_crash_hint",
    "system_server_oom_hint",
)
OUTCOMES = {"success", "failure", "cancelled", "skipped", "unknown"}
AVAILABILITY = {"available", "unavailable", "unknown"}
Executor = Callable[[list[str]], "CommandResult"]


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def _enum(value: Any, allowed: Iterable[str], default: str = "unknown") -> str:
    if not isinstance(value, str):
        return default
    return value if value in allowed else default


def _bounded_float(value: Any, minimum: float, maximum: float) -> float | None:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return None
    try:
        number = float(value)
    except (OverflowError, ValueError):
        return None
    if not math.isfinite(number) or number < minimum or number > maximum:
        return None
    return number


def _bounded_int(value: Any, minimum: int, maximum: int) -> int | None:
    if isinstance(value, bool) or not isinstance(value, int):
        return None
    if value < minimum or value > maximum:
        return None
    return value


def _atomic_json_write(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = (json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n").encode("utf-8")
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb", dir=path.parent, prefix=f".{path.name}.", delete=False
        ) as handle:
            temporary = Path(handle.name)
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            try:
                temporary.unlink()
            except FileNotFoundError:
                pass


class CommandResult:
    __slots__ = ("status", "returncode", "output", "truncated")

    def __init__(
        self,
        status: str,
        output: bytes = b"",
        returncode: int | None = None,
        truncated: bool = False,
    ) -> None:
        self.status = _enum(
            status, {"ok", "error", "timeout", "overflow", "stopped", "cleanup_failed", "unknown"}, "unknown"
        )
        self.returncode = returncode
        self.output = output
        self.truncated = truncated


def _reap_without_collection(process: subprocess.Popen[Any], timeout: float) -> bool:
    """Reap one direct child without reading or accumulating its pipe data."""

    reaped = False
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        reaped = False
    else:
        reaped = True
    finally:
        if process.stdout is not None:
            try:
                process.stdout.close()
            except OSError:
                pass
    return reaped


def _terminate_child(process: subprocess.Popen[Any]) -> bool:
    """Terminate and reap one owned child; never drain an unbounded pipe."""

    if process.poll() is not None:
        if process.stdout is not None:
            try:
                process.stdout.close()
            except OSError:
                pass
        return True
    try:
        process.terminate()
    except ProcessLookupError:
        pass
    if _reap_without_collection(process, 0.5):
        return True
    try:
        process.kill()
    except ProcessLookupError:
        pass
    return _reap_without_collection(process, 0.5)


def run_bounded_command(
    argv: list[str],
    timeout: float,
    max_bytes: int,
    stop_event: threading.Event | None = None,
) -> CommandResult:
    """Run one child with hard runtime/output bounds and exact-child reaping."""

    deadline = time.monotonic() + timeout
    try:
        process = subprocess.Popen(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            close_fds=True,
        )
    except (OSError, ValueError):
        return CommandResult("error")

    assert process.stdout is not None
    chunks: list[bytes] = []
    output_size = 0
    overflow = False
    timed_out = False
    cancelled = False
    cleanup_ok = True
    try:
        while True:
            if stop_event is not None and stop_event.is_set():
                cancelled = True
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                timed_out = True
                break
            ready, _, _ = select.select([process.stdout], [], [], min(0.1, remaining))
            if stop_event is not None and stop_event.is_set():
                cancelled = True
                break
            if not ready:
                continue
            try:
                chunk = os.read(process.stdout.fileno(), LOG_READ_CHUNK_BYTES)
            except (BlockingIOError, InterruptedError):
                continue
            except OSError:
                break
            if not chunk:
                break
            capacity = max_bytes - output_size
            if capacity <= 0:
                overflow = True
                break
            if len(chunk) > capacity:
                chunks.append(chunk[:capacity])
                output_size += capacity
                overflow = True
                break
            chunks.append(chunk)
            output_size += len(chunk)
    finally:
        if cancelled or timed_out or overflow:
            cleanup_ok = _terminate_child(process)
        else:
            try:
                process.wait(timeout=0.5)
            except subprocess.TimeoutExpired:
                cleanup_ok = _terminate_child(process)
            except ChildProcessError:
                cleanup_ok = process.poll() is not None
            finally:
                if process.stdout is not None:
                    try:
                        process.stdout.close()
                    except OSError:
                        pass

    if not cleanup_ok:
        status = "cleanup_failed"
    elif cancelled:
        status = "stopped"
    elif timed_out:
        status = "timeout"
    elif overflow:
        status = "overflow"
    elif process.returncode != 0:
        status = "error"
    else:
        status = "ok"
    return CommandResult(status, b"".join(chunks), process.returncode, overflow)


def _crash_categories(log: bytes, system_server_pids: list[int]) -> dict[str, Any]:
    categories = {name: 0 for name in CRASH_CATEGORIES}
    seen: set[str] = set()
    total_lines = 0
    recognized_lines = 0
    system_server_lines = 0
    pids = set(system_server_pids)

    for raw_line in log.splitlines():
        total_lines += 1
        line = raw_line.rstrip(b"\r")
        digest = hashlib.sha256(line).hexdigest()
        if digest in seen:
            continue
        seen.add(digest)
        decoded = line.decode("utf-8", errors="replace")
        match = THREADTIME_LOG.match(decoded)
        if match is None:
            continue
        recognized_lines += 1
        pid = int(match.group(3))
        level = match.group(5)
        tag = match.group(6)
        if pid not in pids and tag != "system_server":
            continue
        system_server_lines += 1
        if level == "F":
            categories["system_server_fatal"] += 1
        if "FATAL EXCEPTION" in decoded or "WATCHDOG" in decoded.upper():
            categories["system_server_crash_hint"] += 1
        if "OutOfMemoryError" in decoded or "lowmemorykiller" in decoded.lower():
            categories["system_server_oom_hint"] += 1

    if not log:
        status = "observed"
        diagnostic = "ok"
        uncertainty = "bounded empty window; absence does not prove no crash occurred"
    elif recognized_lines == 0:
        status = "unparsed"
        diagnostic = "unrecognized_format"
        uncertainty = "nonzero bounded window had no parser-recognized records; zeros are not confident"
    else:
        status = "observed"
        diagnostic = "ok"
        uncertainty = "bounded window; category absence does not prove no crash occurred"
    return {
        "status": status,
        "diagnostic": diagnostic,
        "categories": categories,
        "total_lines": total_lines,
        "recognized_lines": recognized_lines,
        "system_server_lines": system_server_lines,
        "bounded_tail_bytes": len(log),
        "uncertainty": uncertainty,
    }


def _unknown_crashes(diagnostic: str) -> dict[str, Any]:
    return {
        "status": "unknown",
        "diagnostic": diagnostic,
        "categories": {name: 0 for name in CRASH_CATEGORIES},
        "total_lines": 0,
        "recognized_lines": 0,
        "system_server_lines": 0,
        "bounded_tail_bytes": 0,
        "uncertainty": "bounded system and crash log window unavailable",
    }


def sample_once(
    executor: Executor,
    adb: str,
    serial: str,
    elapsed: float,
    previous_pids: list[int] | None,
) -> tuple[dict[str, Any], list[int] | None, bool]:
    command_count = 7
    transport_failures = 0
    successful_commands = 0
    interrupted = False
    child_cleanup_failed = False

    def probe(argv: list[str]) -> tuple[CommandResult, bool]:
        nonlocal child_cleanup_failed
        if child_cleanup_failed:
            return CommandResult("stopped"), True
        result = executor(argv)
        child_cleanup_failed = child_cleanup_failed or result.status == "cleanup_failed"
        stopped = result.status == "stopped"
        return result, stopped

    def fail(stopped: bool) -> None:
        nonlocal transport_failures, interrupted
        interrupted = interrupted or stopped
        if not stopped:
            transport_failures += 1

    device_result, stopped = probe([adb, "-s", serial, "get-state"])
    state = device_result.output.decode("utf-8", errors="replace").strip()
    device = "available" if device_result.status == "ok" and state == "device" else (
        "offline" if device_result.status == "ok" and state == "offline" else "unknown"
    )
    if device != "unknown":
        successful_commands += 1
    if device == "unknown":
        fail(stopped)

    boot_result, stopped = probe([adb, "-s", serial, "shell", "getprop", "sys.boot_completed"])
    boot_value = boot_result.output.decode("utf-8", errors="replace").strip()
    boot = "true" if boot_result.status == "ok" and boot_value == "1" else (
        "false" if boot_result.status == "ok" and boot_value == "0" else "unknown"
    )
    if boot != "unknown":
        successful_commands += 1
    if boot == "unknown":
        fail(stopped)

    def service_state(service: str) -> str:
        nonlocal successful_commands, transport_failures, interrupted
        result, stopped = probe([adb, "-s", serial, "shell", "service", "check", service])
        value = result.output.decode("utf-8", errors="replace").strip()
        state = "available" if result.status == "ok" and value == f"Service {service}: found" else (
            "unavailable" if result.status == "ok" and value == f"Service {service}: not found" else "unknown"
        )
        if state == "unknown":
            interrupted = interrupted or stopped
            if not stopped:
                transport_failures += 1
        else:
            successful_commands += 1
        return state

    package_service = service_state("package")
    activity_service = service_state("activity")
    input_service = service_state("input")

    pid_result, stopped = probe(
        [adb, "-s", serial, "shell", "pidof", "system_server"]
    )
    pid_text = pid_result.output.decode("utf-8", errors="replace").strip()
    pids: list[int] = []
    pid_status = "unknown"
    if pid_result.status == "ok":
        fields = pid_text.split()
        if not fields:
            pids, pid_status = [], "observed"
            successful_commands += 1
        elif len(fields) <= 16 and all(re.fullmatch(r"[1-9]\d*", field) for field in fields):
            pids = sorted({int(field) for field in fields})
            pid_status = "observed" if all(pid <= MAX_PID for pid in pids) else "unknown"
            if pid_status == "observed":
                successful_commands += 1
    if pid_status == "unknown":
        fail(stopped)

    log_result, log_stopped = probe([
        adb, "-s", serial, "shell", "logcat", "-d", "-v", "threadtime",
        "-b", "system", "-b", "crash", "-t", "256",
    ])
    interrupted = interrupted or log_stopped
    if log_result.status == "ok":
        bounded_log = log_result.output[:MAX_LOG_BYTES]
        crashes = _crash_categories(bounded_log, pids)
        if log_result.truncated or len(log_result.output) > MAX_LOG_BYTES:
            crashes["diagnostic"] = "truncated"
            crashes["uncertainty"] = "bounded output was truncated before the full window"
        if crashes["status"] == "observed":
            successful_commands += 1
        else:
            transport_failures += 1
    else:
        crashes = _unknown_crashes(log_result.status)
        if not log_stopped:
            transport_failures += 1

    previous = previous_pids
    pid_changed = (
        pid_status == "observed"
        and previous is not None
        and set(pids) != set(previous)
    )
    field_count_changed = (
        len(set(pids).symmetric_difference(previous))
        if pid_status == "observed" and previous is not None
        else 0
    )
    if interrupted and successful_commands == 0:
        collection_status = "interrupted"
    elif transport_failures == 0 and successful_commands == command_count:
        collection_status = "observed"
    elif successful_commands:
        collection_status = "partial"
    else:
        collection_status = "unknown"

    sample = {
        "record_type": "sample",
        "elapsed_seconds": round(elapsed, 3),
        "timestamp_utc": utc_now(),
        "device": device,
        "boot_completed": boot,
        "package_service": package_service,
        "activity_service": activity_service,
        "input_service": input_service,
        "system_server_pids": pids,
        "system_server_pid_status": pid_status,
        "system_server_pid_changed": pid_changed,
        "system_server_pid_field_count_changed": field_count_changed,
        "child_cleanup_failed": child_cleanup_failed,
        "crashes": crashes,
        "collection_status": collection_status,
        "unknown_command_count": transport_failures,
    }
    # The final result means "no semantically successful measurement", not "every field is unknown".
    return sample, pids if pid_status == "observed" else previous_pids, successful_commands == 0


class FrameworkMonitor:
    def __init__(
        self,
        output: Path,
        adb: str,
        interval: float,
        lifetime: float,
        command_timeout: float,
    ) -> None:
        self.output = output
        self.adb = adb
        self.serial = DEFAULT_SERIAL
        self.interval = interval
        self.lifetime = lifetime
        self.command_timeout = command_timeout
        self.stop_event = threading.Event()
        self.started = time.monotonic()
        self.deadline = self.started + lifetime
        self.output_truncated = False

    def request_stop(self, *_args: Any) -> None:
        self.stop_event.set()

    def _executor(self, argv: list[str]) -> CommandResult:
        if self.stop_event.is_set():
            return CommandResult("stopped")
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            return CommandResult("stopped")
        return run_bounded_command(
            argv,
            min(self.command_timeout, remaining),
            MAX_LOG_BYTES,
            self.stop_event,
        )

    def _write_json_line(self, handle: Any, value: dict[str, Any]) -> bool:
        payload = (json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n").encode("utf-8")
        position = handle.tell()
        if position >= MAX_OUTPUT_BYTES or position + len(payload) > MAX_OUTPUT_BYTES:
            self.output_truncated = True
            return False
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
        return True

    def run(self) -> int:
        self.output.parent.mkdir(parents=True, exist_ok=True)
        previous_signal = signal.signal(signal.SIGTERM, self.request_stop)
        previous_pids: list[int] | None = None
        sample_count = 0
        no_successful_measurements = True
        child_cleanup_failed = False
        reason = "deadline"
        failure_reason = "none"
        try:
            with self.output.open("ab") as handle:
                while not self.stop_event.is_set() and time.monotonic() < self.deadline:
                    elapsed = time.monotonic() - self.started
                    try:
                        sample, previous_pids, measurements_absent = sample_once(
                            self._executor, self.adb, self.serial, elapsed, previous_pids
                        )
                    except Exception:
                        failure_reason = "sampling_failure"
                        reason = "collection_failure"
                        break
                    sample_count += 1
                    if not measurements_absent:
                        no_successful_measurements = False
                    if not self._write_json_line(handle, sample):
                        failure_reason = "output_bound"
                        reason = "collection_failure"
                        break
                    child_cleanup_failed = child_cleanup_failed or sample["child_cleanup_failed"] is True
                    if child_cleanup_failed:
                        failure_reason = "child_cleanup_failure"
                        reason = "collection_failure"
                        break
                    if self.stop_event.is_set() or time.monotonic() >= self.deadline:
                        break
                    self.stop_event.wait(min(self.interval, self.deadline - time.monotonic()))
                if reason != "collection_failure":
                    if self.stop_event.is_set():
                        reason = "requested_stop"
                    elif time.monotonic() >= self.deadline:
                        reason = "deadline"
                    else:
                        reason = "requested_stop"
                if sample_count == 0 and failure_reason == "none":
                    reason = "collection_failure"
                    failure_reason = "no_samples"
                elif sample_count == 0:
                    reason = "collection_failure"
                    failure_reason = "no_samples"
                elif child_cleanup_failed:
                    reason = "collection_failure"
                    failure_reason = "child_cleanup_failure"
                elif no_successful_measurements:
                    if reason == "deadline":
                        failure_reason = "no_successful_measurements"
                    else:
                        reason = "collection_failure"
                        failure_reason = "all_samples_unknown"
        except OSError:
            failure_reason = "output_failure"
            reason = "collection_failure"
        finally:
            signal.signal(signal.SIGTERM, previous_signal)

        completion = {
            "record_type": "completion",
            "completion_reason": reason,
            "sample_count": sample_count,
            "started_monotonic_seconds": round(self.started, 3),
            "finished_monotonic_seconds": round(time.monotonic(), 3),
            "output_truncated": self.output_truncated,
            "collection_failure_reason": failure_reason,
        }
        try:
            with self.output.open("ab") as handle:
                self._write_json_line(handle, completion)
        except OSError:
            return 2
        return 0 if reason in {"requested_stop", "deadline"} else 2


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _process_start_token(pid: int) -> str | None:
    try:
        stat = Path(f"/proc/{pid}/stat").read_text(encoding="ascii")
    except (OSError, UnicodeError):
        return None
    close = stat.rfind(")")
    if close < 0:
        return None
    fields = stat[close + 2 :].split()
    if len(fields) < 20:
        return None
    return fields[19]


def _process_exists(pid: int) -> bool:
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def _process_is_zombie(pid: int) -> bool:
    try:
        stat = Path(f"/proc/{pid}/stat").read_text(encoding="ascii")
    except (OSError, UnicodeError):
        try:
            state = subprocess.run(
                ["ps", "-o", "stat=", "-p", str(pid)],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                timeout=1,
                check=False,
            ).stdout.decode("utf-8", errors="replace").strip()
        except (OSError, subprocess.SubprocessError):
            return False
        return state.startswith("Z")
    close = stat.rfind(")")
    return close >= 0 and stat[close + 2 :].split()[0] == "Z"


def _signal_owned(pid: int, signal_number: int) -> bool:
    try:
        os.kill(pid, signal_number)
        return True
    except (ProcessLookupError, PermissionError):
        return False


def _wait_for_exit(pid: int, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not _process_exists(pid) or _process_is_zombie(pid):
            return True
        time.sleep(0.05)
    return not _process_exists(pid) or _process_is_zombie(pid)


def _stop_owned_process(pid: int, token: str) -> str:
    if not _process_exists(pid):
        return "already_exited"
    if _process_start_token(pid) != token:
        return "identity_mismatch"
    if not _signal_owned(pid, signal.SIGTERM):
        return "signal_failure"
    if _wait_for_exit(pid, STOP_GRACE_SECONDS):
        return "stopped"
    # Recheck identity immediately before any escalation.
    if _process_start_token(pid) != token:
        return "identity_mismatch"
    if not _signal_owned(pid, signal.SIGKILL):
        return "signal_failure"
    return "stopped" if _wait_for_exit(pid, KILL_GRACE_SECONDS) else "stop_timeout"


def _start_detached(arguments: list[str], cwd: str) -> subprocess.Popen[Any]:
    return subprocess.Popen(
        arguments,
        cwd=cwd,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        close_fds=True,
        start_new_session=True,
    )


def _write_ownership(
    path: Path,
    *,
    pid: int,
    process_start_token: str,
    expected_script_sha256: str,
    output: Path,
    interval: float,
    lifetime: float,
    command_timeout: float,
    phone_outcome: str,
) -> None:
    value = {
        "schema": OWNERSHIP_SCHEMA,
        "pid": pid,
        "process_start_token": process_start_token,
        "expected_script_sha256": expected_script_sha256,
        "output": str(output),
        "interval_seconds": interval,
        "lifetime_seconds": lifetime,
        "command_timeout_seconds": command_timeout,
        "phase": "pre-tablet-emulator",
        "phone_outcome": _enum(phone_outcome, OUTCOMES),
        "created_utc": utc_now(),
    }
    _atomic_json_write(path, value)


def _load_ownership(path: Path) -> dict[str, Any] | None:
    try:
        raw = _read_bounded(path)
        if raw is None or len(raw) > MAX_OUTPUT_BYTES:
            return None
        value = json.loads(raw)
    except (OSError, UnicodeError, json.JSONDecodeError):
        return None
    if not isinstance(value, dict) or set(value) != {
        "schema", "pid", "process_start_token", "expected_script_sha256", "output",
        "interval_seconds", "lifetime_seconds", "command_timeout_seconds", "phase",
        "phone_outcome", "created_utc",
    }:
        return None
    if value.get("schema") != OWNERSHIP_SCHEMA:
        return None
    pid = _bounded_int(value.get("pid"), 1, MAX_PID)
    token = value.get("process_start_token")
    digest = value.get("expected_script_sha256")
    output = value.get("output")
    interval = _bounded_float(value.get("interval_seconds"), 0.01, MAX_INTERVAL_SECONDS)
    lifetime = _bounded_float(value.get("lifetime_seconds"), 0.05, MAX_LIFETIME_SECONDS)
    timeout = _bounded_float(value.get("command_timeout_seconds"), 0.05, MAX_COMMAND_TIMEOUT_SECONDS)
    created = value.get("created_utc")
    if pid is None or not isinstance(token, str) or not re.fullmatch(r"\d{1,20}", token):
        return None
    if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
        return None
    if interval is None or lifetime is None or timeout is None:
        return None
    if (
        not isinstance(output, str)
        or not output
        or len(output) > 1024
        or "\x00" in output
        or not Path(output).is_absolute()
    ):
        return None
    if not isinstance(created, str) or TIMESTAMP.fullmatch(created) is None:
        return None
    if value.get("phase") != "pre-tablet-emulator":
        return None
    phone_outcome = value.get("phone_outcome")
    if not isinstance(phone_outcome, str) or phone_outcome not in OUTCOMES:
        return None
    return {
        "schema": OWNERSHIP_SCHEMA,
        "pid": pid,
        "process_start_token": token,
        "expected_script_sha256": digest,
        "output": output,
        "interval_seconds": interval,
        "lifetime_seconds": lifetime,
        "command_timeout_seconds": timeout,
        "phase": value["phase"],
        "phone_outcome": phone_outcome,
        "created_utc": created,
    }


def _sanitize_crashes(value: Any) -> dict[str, Any] | None:
    if not isinstance(value, dict):
        return None
    categories: dict[str, int] = {}
    for name in CRASH_CATEGORIES:
        count = _bounded_int(value.get("categories", {}).get(name), 0, 100000)
        if count is None:
            return None
        categories[name] = count
    status = _enum(value.get("status"), {"observed", "unparsed", "unknown"}, "unknown")
    diagnostic = _enum(
        value.get("diagnostic"),
        {"ok", "truncated", "unrecognized_format", "error", "timeout", "overflow", "stopped", "unknown"},
        "unknown",
    )
    total = _bounded_int(value.get("total_lines"), 0, 100000)
    recognized = _bounded_int(value.get("recognized_lines"), 0, 100000)
    system = _bounded_int(value.get("system_server_lines"), 0, 100000)
    tail = _bounded_int(value.get("bounded_tail_bytes"), 0, MAX_LOG_BYTES)
    if total is None or recognized is None or system is None or tail is None:
        return None
    if status == "unknown":
        uncertainty = "bounded system and crash log window unavailable"
    elif status == "unparsed":
        uncertainty = "nonzero bounded window had no parser-recognized records; zeros are not confident"
    else:
        uncertainty = "bounded window; category absence does not prove no crash occurred"
    return {
        "status": status,
        "diagnostic": diagnostic,
        "categories": categories,
        "total_lines": total,
        "recognized_lines": recognized,
        "system_server_lines": system,
        "bounded_tail_bytes": tail,
        "uncertainty": uncertainty,
    }


def _sanitize_sample(value: dict[str, Any]) -> dict[str, Any] | None:
    try:
        elapsed = _bounded_float(value.get("elapsed_seconds"), 0, MAX_LIFETIME_SECONDS * 2)
        timestamp = value.get("timestamp_utc")
        if elapsed is None or not isinstance(timestamp, str) or TIMESTAMP.fullmatch(timestamp) is None:
            return None
        pids_value = value.get("system_server_pids")
        if not isinstance(pids_value, list) or len(pids_value) > 16:
            return None
        pids: list[int] = []
        for pid in pids_value:
            clean_pid = _bounded_int(pid, 1, MAX_PID)
            if clean_pid is None:
                return None
            pids.append(clean_pid)
        pid_status = _enum(value.get("system_server_pid_status"), {"observed", "unknown"}, "unknown")
        pid_changed = value.get("system_server_pid_changed")
        cleanup_failed = value.get("child_cleanup_failed")
        field_changes = _bounded_int(value.get("system_server_pid_field_count_changed"), 0, 32)
        unknown_commands = _bounded_int(value.get("unknown_command_count"), 0, 7)
        crashes = _sanitize_crashes(value.get("crashes"))
        collection_status = _enum(
            value.get("collection_status"),
            {"observed", "partial", "unknown", "interrupted"},
            "unknown",
        )
        if not isinstance(pid_changed, bool) or not isinstance(cleanup_failed, bool):
            return None
        if pid_status == "unknown" and pids:
            return None
        if field_changes is None or unknown_commands is None or crashes is None:
            return None
        return {
            "record_type": "sample",
            "elapsed_seconds": elapsed,
            "timestamp_utc": timestamp,
            "device": _enum(value.get("device"), {"available", "offline", "unknown"}),
            "boot_completed": _enum(value.get("boot_completed"), {"true", "false", "unknown"}),
            "package_service": _enum(value.get("package_service"), AVAILABILITY),
            "activity_service": _enum(value.get("activity_service"), AVAILABILITY),
            "input_service": _enum(value.get("input_service"), AVAILABILITY),
            "system_server_pids": pids,
            "system_server_pid_status": pid_status,
            "system_server_pid_changed": pid_changed,
            "system_server_pid_field_count_changed": field_changes,
            "child_cleanup_failed": cleanup_failed,
            "crashes": crashes,
            "collection_status": collection_status,
            "unknown_command_count": unknown_commands,
        }
    except (TypeError, ValueError, AttributeError):
        return None


def _read_bounded(path: Path) -> bytes | None:
    try:
        with path.open("rb") as handle:
            return handle.read(MAX_OUTPUT_BYTES + 1)
    except OSError:
        return None


def _parse_monitor_output(path: Path) -> tuple[list[dict[str, Any]], dict[str, Any] | None, dict[str, int]]:
    samples: list[dict[str, Any]] = []
    completion: dict[str, Any] | None = None
    rejected = {"malformed_records": 0, "oversize_records": 0}
    raw = _read_bounded(path)
    if raw is None:
        return samples, completion, rejected
    if len(raw) > MAX_OUTPUT_BYTES:
        rejected["oversize_records"] += 1
        return samples, completion, rejected
    for line in raw.splitlines():
        if not line.strip():
            continue
        try:
            value = json.loads(line)
        except (UnicodeError, json.JSONDecodeError):
            rejected["malformed_records"] += 1
            continue
        if not isinstance(value, dict):
            rejected["malformed_records"] += 1
            continue
        if value.get("record_type") == "sample":
            sample = _sanitize_sample(value)
            if sample is None:
                rejected["malformed_records"] += 1
            else:
                samples.append(sample)
        elif value.get("record_type") == "completion":
            count = _bounded_int(value.get("sample_count"), 0, 10000)
            if count is None:
                rejected["malformed_records"] += 1
                continue
            completion = {
                "completion_reason": _enum(
                    value.get("completion_reason"),
                    {"requested_stop", "deadline", "collection_failure"},
                    "collection_failure",
                ),
                "sample_count": count,
                "output_truncated": value.get("output_truncated") is True,
                "collection_failure_reason": _enum(
                    value.get("collection_failure_reason"),
                    {
                        "none", "sampling_failure", "output_bound", "output_failure",
                        "no_samples", "all_samples_unknown",
                        "no_successful_measurements", "child_cleanup_failure",
                    },
                    "all_samples_unknown",
                ),
            }
        else:
            rejected["malformed_records"] += 1
    return samples, completion, rejected


def _summarize_samples(samples: list[dict[str, Any]]) -> dict[str, Any]:
    peaks = {name: 0 for name in CRASH_CATEGORIES}
    unparsed_windows = 0
    unknown_windows = 0
    changes = 0
    previous: set[int] | None = None
    unknown_commands = 0
    for sample in samples:
        crashes = sample["crashes"]
        for name in CRASH_CATEGORIES:
            peaks[name] = max(peaks[name], crashes["categories"][name])
        if crashes["status"] == "unparsed":
            unparsed_windows += 1
        if crashes["status"] == "unknown":
            unknown_windows += 1
        if sample["system_server_pid_status"] == "observed":
            pids = set(sample["system_server_pids"])
            if previous is not None and pids != previous:
                changes += 1
            previous = pids
        unknown_commands += sample["unknown_command_count"]
    if not samples:
        return {
            "first_sample_utc": None,
            "last_sample_utc": None,
            "coverage_seconds": 0.0,
            "service_observations": 0,
            "system_server_pid_changes": 0,
            "crash_category_peak_observations": peaks,
            "unparsed_crash_windows": 0,
            "unknown_crash_windows": 0,
            "unknown_command_count_total": 0,
            "aggregation": "maximum_per_bounded_window_noninflating",
        }
    return {
        "first_sample_utc": samples[0]["timestamp_utc"],
        "last_sample_utc": samples[-1]["timestamp_utc"],
        "coverage_seconds": max(0.0, samples[-1]["elapsed_seconds"] - samples[0]["elapsed_seconds"]),
        "service_observations": len(samples),
        "system_server_pid_changes": changes,
        "crash_category_peak_observations": peaks,
        "unparsed_crash_windows": unparsed_windows,
        "unknown_crash_windows": unknown_windows,
        "unknown_command_count_total": unknown_commands,
        "aggregation": "maximum_per_bounded_window_noninflating",
    }


def _fallback_result(
    status: str,
    diagnostic: str,
    phone_outcome: str,
    tablet_outcome: str,
) -> dict[str, Any]:
    return {
        "schema": RESULT_SCHEMA,
        "status": status,
        "completion_reason": "collection_failure",
        "collection_failure_reason": diagnostic,
        "phase": "unknown",
        "phone_outcome": _enum(phone_outcome, OUTCOMES),
        "tablet_outcome": _enum(tablet_outcome, OUTCOMES),
        "coverage": {
            "started_utc": None,
            "finished_utc": utc_now(),
            "sample_count": 0,
            "coverage_complete": False,
            "gap_reason": "no_sanitized_samples",
            "deadline_reached_before_finalization": False,
        },
        "monitor": {
            "interval_seconds": None,
            "configured_lifetime_seconds": None,
            "command_timeout_seconds": None,
            "output_truncated": False,
        },
        "diagnostics": _summarize_samples([]),
        "samples": [],
        "output_record_rejections": {"malformed_records": 0, "oversize_records": 0},
    }


def _build_final_result(
    ownership: dict[str, Any],
    monitor_output: Path,
    tablet_outcome: str,
) -> tuple[dict[str, Any], bool]:
    samples, completion, rejected = _parse_monitor_output(monitor_output)
    status = "completed"
    failure_reason = "none"
    if completion is None:
        status = "collection_failure"
        failure_reason = "no_completion_record"
    elif completion["completion_reason"] == "collection_failure":
        status = "collection_failure"
        failure_reason = completion["collection_failure_reason"]
    elif not samples:
        status = "collection_failure"
        failure_reason = "no_sanitized_samples"
    elif not any(sample["collection_status"] in {"observed", "partial"} for sample in samples):
        status = "collection_failure"
        failure_reason = "no_successful_measurements"
    if completion is not None and completion["output_truncated"] and status == "completed":
        status = "collection_failure"
        failure_reason = "output_bound"

    reason = completion["completion_reason"] if completion else "collection_failure"
    deadline_before_finalization = reason == "deadline"
    output_truncated = bool(completion and completion["output_truncated"])
    completion_count_matches = completion is not None and completion["sample_count"] == len(samples)
    records_rejected = (
        rejected["malformed_records"] > 0
        or rejected["oversize_records"] > 0
        or not completion_count_matches
    )
    all_samples_observed = bool(samples) and all(
        sample["collection_status"] == "observed" for sample in samples
    )
    coverage_complete = (
        reason == "requested_stop"
        and status == "completed"
        and all_samples_observed
        and not output_truncated
        and not records_rejected
    )
    gap = "none"
    if completion is None:
        gap = "no_completion_record"
    elif deadline_before_finalization:
        gap = "deadline_before_action_finalization"
    elif not samples:
        gap = "no_sanitized_samples"
    elif output_truncated:
        gap = "output_truncated"
    elif records_rejected:
        gap = "output_records_rejected"
    elif not coverage_complete:
        gap = "partial_observation"
    result = {
        "schema": RESULT_SCHEMA,
        "status": status,
        "completion_reason": reason,
        "collection_failure_reason": failure_reason,
        "phase": ownership["phase"],
        "phone_outcome": ownership["phone_outcome"],
        "tablet_outcome": _enum(tablet_outcome, OUTCOMES),
        "coverage": {
            "started_utc": samples[0]["timestamp_utc"] if samples else None,
            "finished_utc": samples[-1]["timestamp_utc"] if samples else utc_now(),
            "sample_count": len(samples),
            "coverage_complete": coverage_complete,
            "gap_reason": gap,
            "deadline_reached_before_finalization": deadline_before_finalization,
        },
        "monitor": {
            "interval_seconds": ownership["interval_seconds"],
            "configured_lifetime_seconds": ownership["lifetime_seconds"],
            "command_timeout_seconds": ownership["command_timeout_seconds"],
            "output_truncated": output_truncated,
        },
        "diagnostics": _summarize_samples(samples),
        "samples": samples,
        "output_record_rejections": rejected,
    }
    return result, status != "completed"


def _start(args: argparse.Namespace) -> int:
    script = Path(__file__).resolve()
    output = Path(args.output).absolute()
    ownership_path = Path(args.ownership).absolute()
    interval = _bounded_float(args.interval, 0.01, MAX_INTERVAL_SECONDS)
    lifetime = _bounded_float(args.lifetime, 0.05, MAX_LIFETIME_SECONDS)
    timeout = _bounded_float(args.command_timeout, 0.05, MAX_COMMAND_TIMEOUT_SECONDS)
    phone_outcome = _enum(args.phone_outcome, OUTCOMES)
    if interval is None or lifetime is None or timeout is None:
        print("diagnostic monitor launch failed: invalid_bounds", file=sys.stderr)
        return 2
    try:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(b"")
        arguments = [
            sys.executable,
            str(script),
            "monitor",
            "--output", str(output),
            "--adb", args.adb,
            "--interval", str(interval),
            "--lifetime", str(lifetime),
            "--command-timeout", str(timeout),
        ]
        process = _start_detached(arguments, os.getcwd())
    except OSError:
        print("diagnostic monitor launch failed: launch_failure", file=sys.stderr)
        return 2
    time.sleep(0.02)
    token = _process_start_token(process.pid)
    alive = _process_exists(process.pid) and not _process_is_zombie(process.pid)
    if token is None or not alive:
        _terminate_child(process)
        print("diagnostic monitor launch failed: startup_failure", file=sys.stderr)
        return 2
    try:
        _write_ownership(
            ownership_path,
            pid=process.pid,
            process_start_token=token,
            expected_script_sha256=_sha256_file(script),
            output=output,
            interval=interval,
            lifetime=lifetime,
            command_timeout=timeout,
            phone_outcome=phone_outcome,
        )
    except OSError:
        _stop_owned_process(process.pid, token)
        print("diagnostic monitor launch failed: ownership_write_failure", file=sys.stderr)
        return 2
    return 0


def _stop(args: argparse.Namespace) -> int:
    ownership = _load_ownership(Path(args.ownership).absolute())
    output = Path(args.output).absolute()
    tablet_outcome = _enum(args.tablet_outcome, OUTCOMES)
    if ownership is None:
        result = _fallback_result(
            "ownership_metadata_invalid", "ownership_metadata_invalid", "unknown", tablet_outcome
        )
        _atomic_json_write(output, result)
        print("diagnostic finalization failed: ownership_metadata_invalid", file=sys.stderr)
        return 2

    monitor_output = Path(ownership["output"])
    stop_status = _stop_owned_process(ownership["pid"], ownership["process_start_token"])
    if stop_status not in {"already_exited", "stopped"}:
        result = _fallback_result(
            f"process_{stop_status}",
            f"process_{stop_status}",
            ownership["phone_outcome"],
            tablet_outcome,
        )
        _atomic_json_write(output, result)
        print(f"diagnostic finalization failed: {stop_status}", file=sys.stderr)
        return 2

    result, explicit_failure = _build_final_result(ownership, monitor_output, tablet_outcome)
    if _sha256_file(Path(__file__).resolve()) != ownership["expected_script_sha256"]:
        result["status"] = "collection_failure"
        result["collection_failure_reason"] = "expected_script_mismatch"
        explicit_failure = True
    _atomic_json_write(output, result)
    if explicit_failure:
        print("diagnostic finalization failed: collection_incomplete", file=sys.stderr)
        return 2
    return 0


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Collect bounded Android framework diagnostics")
    commands = parser.add_subparsers(dest="command", required=True)

    start = commands.add_parser("start", help="start a detached bounded monitor")
    start.add_argument("--ownership", required=True)
    start.add_argument("--output", required=True)
    start.add_argument("--adb", default="adb")
    start.add_argument("--interval", type=float, default=DEFAULT_INTERVAL_SECONDS)
    start.add_argument("--lifetime", type=float, default=DEFAULT_LIFETIME_SECONDS)
    start.add_argument("--command-timeout", type=float, default=DEFAULT_COMMAND_TIMEOUT_SECONDS)
    start.add_argument("--phone-outcome", default="unknown")
    start.set_defaults(handler=_start)

    monitor = commands.add_parser("monitor", help="internal bounded sampling loop")
    monitor.add_argument("--output", required=True)
    monitor.add_argument("--adb", default="adb")
    monitor.add_argument("--interval", type=float, default=DEFAULT_INTERVAL_SECONDS)
    monitor.add_argument("--lifetime", type=float, default=DEFAULT_LIFETIME_SECONDS)
    monitor.add_argument("--command-timeout", type=float, default=DEFAULT_COMMAND_TIMEOUT_SECONDS)
    monitor.set_defaults(handler=lambda args: FrameworkMonitor(
        Path(args.output).absolute(), args.adb,
        _bounded_float(args.interval, 0.01, MAX_INTERVAL_SECONDS) or MAX_INTERVAL_SECONDS,
        _bounded_float(args.lifetime, 0.05, MAX_LIFETIME_SECONDS) or MAX_LIFETIME_SECONDS,
        _bounded_float(args.command_timeout, 0.05, MAX_COMMAND_TIMEOUT_SECONDS) or MAX_COMMAND_TIMEOUT_SECONDS,
    ).run())

    stop = commands.add_parser("stop", help="stop the owned monitor and finalize sanitized output")
    stop.add_argument("--ownership", required=True)
    stop.add_argument("--output", required=True)
    stop.add_argument("--tablet-outcome", required=True)
    stop.set_defaults(handler=_stop)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        return int(args.handler(args))
    except OSError:
        print("diagnostics operation failed: output_failure", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
