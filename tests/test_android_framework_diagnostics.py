import argparse
import importlib.util
import io
import json
import os
import stat
import subprocess
import sys
import tempfile
import textwrap
import threading
import time
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "src/scripts/android-framework-diagnostics.py"
_spec = importlib.util.spec_from_file_location("android_framework_diagnostics", SCRIPT)
diagnostics = importlib.util.module_from_spec(_spec)
assert _spec.loader is not None
_spec.loader.exec_module(diagnostics)

REAL_THREADTIME = (
    b"09-10 12:00:00.000  1200  1200 F system_server : TOPSECRET-VALUE FATAL EXCEPTION\n"
    b"09-10 12:00:00.100  1200  1200 E system_server : OutOfMemoryError\n"
    b"09-10 12:00:00.200  1201  1201 I unknown : account=topsecret path=/private/value\n"
)


class StaticScriptTest(unittest.TestCase):
    def test_source_compiles_without_execution_side_effects(self):
        compile(SCRIPT.read_text(encoding="utf-8"), str(SCRIPT), "exec")


class BoundedChildTest(unittest.TestCase):
    def test_output_overflow_terminates_and_reaps_child(self):
        with tempfile.TemporaryDirectory() as directory:
            helper = Path(directory) / "writer.py"
            helper.write_text("print('x' * 1024)\n", encoding="utf-8")
            result = diagnostics.run_bounded_command(
                [sys.executable, str(helper)], timeout=3, max_bytes=128
            )
            self.assertEqual("overflow", result.status)
            self.assertTrue(result.truncated)
            self.assertEqual(128, len(result.output))
            self.assertIsNotNone(result.returncode)

    def test_command_timeout_terminates_and_reaps_hanging_child(self):
        with tempfile.TemporaryDirectory() as directory:
            helper = Path(directory) / "slow.py"
            helper.write_text("import time; time.sleep(2)\n", encoding="utf-8")
            result = diagnostics.run_bounded_command(
                [sys.executable, str(helper)], timeout=0.1, max_bytes=128
            )
            self.assertEqual("timeout", result.status)
            self.assertEqual(b"", result.output)
            self.assertIsNotNone(result.returncode)

    def test_stop_event_terminates_and_reaps_active_child(self):
        with tempfile.TemporaryDirectory() as directory:
            helper = Path(directory) / "slow.py"
            helper.write_text("import time; time.sleep(2)\n", encoding="utf-8")
            stop = threading.Event()
            timer = threading.Timer(0.1, stop.set)
            timer.start()
            try:
                result = diagnostics.run_bounded_command(
                    [sys.executable, str(helper)], timeout=5, max_bytes=128, stop_event=stop
                )
            finally:
                timer.join()
            self.assertEqual("stopped", result.status)
            self.assertIsNotNone(result.returncode)


    def test_terminate_child_escalates_and_confirms_reap(self):
        process = mock.Mock(pid=1234, poll=lambda: None, stdout=None)
        process.wait.side_effect = [subprocess.TimeoutExpired(cmd="fake", timeout=0.5), None]
        self.assertTrue(diagnostics._terminate_child(process))
        process.terminate.assert_called_once_with()
        process.kill.assert_called_once_with()

    def test_terminate_child_reports_deterministic_reap_failure(self):
        process = mock.Mock(pid=1235, poll=lambda: None, stdout=None)
        process.wait.side_effect = subprocess.TimeoutExpired(cmd="fake", timeout=0.5)
        self.assertFalse(diagnostics._terminate_child(process))
        process.terminate.assert_called_once_with()
        process.kill.assert_called_once_with()

    def test_cleanup_failure_prevents_further_sampling_commands(self):
        executor = FakeExecutor([diagnostics.CommandResult("cleanup_failed")])
        sample, _, all_unknown = diagnostics.sample_once(executor, "adb", "emulator-5554", 0, [])
        self.assertTrue(sample["child_cleanup_failed"])
        self.assertEqual(1, executor.index)
        self.assertEqual("interrupted", sample["collection_status"])
        self.assertTrue(all_unknown)

    def test_cleanup_failure_after_success_stops_remaining_probes(self):
        executor = FakeExecutor([
            diagnostics.CommandResult("ok", b"device", 0),
            diagnostics.CommandResult("cleanup_failed"),
        ])
        sample, _, measurements_absent = diagnostics.sample_once(
            executor, "adb", "emulator-5554", 0, []
        )
        self.assertTrue(sample["child_cleanup_failed"])
        self.assertEqual(2, executor.index)
        self.assertEqual("partial", sample["collection_status"])
        self.assertFalse(measurements_absent)

    def test_monitor_cleanup_failure_stops_probes_and_preserves_reason(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "monitor.jsonl"
            monitor = diagnostics.FrameworkMonitor(output, "adb", 0.01, 1, 0.1)
            calls = []

            def cleanup_executor(argv):
                calls.append(argv)
                return diagnostics.CommandResult("cleanup_failed")

            monitor._executor = cleanup_executor
            exit_code = monitor.run()
            samples, completion, rejected = diagnostics._parse_monitor_output(output)
            self.assertEqual(2, exit_code)
            self.assertEqual(1, len(calls))
            self.assertEqual(1, len(samples))
            self.assertTrue(samples[0]["child_cleanup_failed"])
            self.assertEqual(0, rejected["malformed_records"])
            self.assertEqual("collection_failure", completion["completion_reason"])
            self.assertEqual("child_cleanup_failure", completion["collection_failure_reason"])

    def test_monitor_cleanup_after_success_stops_probes_and_preserves_reason(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "monitor.jsonl"
            monitor = diagnostics.FrameworkMonitor(output, "adb", 0.01, 1, 0.1)
            calls = []

            def cleanup_after_success(argv):
                calls.append(argv)
                if len(calls) == 1:
                    return diagnostics.CommandResult("ok", b"device", 0)
                return diagnostics.CommandResult("cleanup_failed")

            monitor._executor = cleanup_after_success
            exit_code = monitor.run()
            samples, completion, rejected = diagnostics._parse_monitor_output(output)
            self.assertEqual(2, exit_code)
            self.assertEqual(2, len(calls))
            self.assertEqual(1, len(samples))
            self.assertTrue(samples[0]["child_cleanup_failed"])
            self.assertEqual("partial", samples[0]["collection_status"])
            self.assertEqual(0, rejected["malformed_records"])
            self.assertEqual("collection_failure", completion["completion_reason"])
            self.assertEqual("child_cleanup_failure", completion["collection_failure_reason"])

    def test_deadline_without_successful_measurements_is_not_generic_unknown(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "monitor.jsonl"
            monitor = diagnostics.FrameworkMonitor(output, "adb", 0.01, 0.03, 0.1)
            monitor._executor = lambda argv: diagnostics.CommandResult("error")
            exit_code = monitor.run()
            samples, completion, rejected = diagnostics._parse_monitor_output(output)
            self.assertEqual(0, exit_code)
            self.assertGreater(len(samples), 0)
            self.assertEqual("deadline", completion["completion_reason"])
            self.assertEqual(
                "no_successful_measurements", completion["collection_failure_reason"]
            )
            self.assertEqual(0, rejected["malformed_records"])

class FakeExecutor:
    def __init__(self, results):
        self.results = list(results)
        self.index = 0
        self.commands: list[list[str]] = []
        self.last_was_stopped = False

    def __call__(self, argv):
        self.commands.append(argv)
        if self.index < len(self.results):
            result = self.results[self.index]
        else:
            result = diagnostics.CommandResult("error")
        self.index += 1
        self.last_was_stopped = result.status == "stopped"
        return result


def healthy_results():
    return [
        diagnostics.CommandResult("ok", b"device", 0),
        diagnostics.CommandResult("ok", b"1", 0),
        diagnostics.CommandResult("ok", b"Service package: found", 0),
        diagnostics.CommandResult("ok", b"Service activity: found", 0),
        diagnostics.CommandResult("ok", b"Service input: found", 0),
        diagnostics.CommandResult("ok", b"1200", 0),
        diagnostics.CommandResult("ok", REAL_THREADTIME, 0),
    ]


class SamplingTest(unittest.TestCase):
    def test_binder_services_are_observed_without_app_probes(self):
        executor = FakeExecutor(healthy_results())
        sample, pids, all_unknown = diagnostics.sample_once(executor, "adb", diagnostics.DEFAULT_SERIAL, 1.25, [1200])
        self.assertEqual("available", sample["device"])
        self.assertEqual("true", sample["boot_completed"])
        self.assertEqual("available", sample["package_service"])
        self.assertEqual("available", sample["activity_service"])
        self.assertEqual("available", sample["input_service"])
        self.assertEqual([1200], pids)
        self.assertFalse(all_unknown)
        self.assertEqual("observed", sample["collection_status"])
        serialized = json.dumps(sample)
        self.assertNotIn("TOPSECRET-VALUE", serialized)
        self.assertNotIn("topsecret", serialized)
        self.assertNotIn("resolve-activity", serialized)
        self.assertNotIn("pm path", serialized)

    def test_missing_device_transport_error_is_unknown_not_absent(self):
        results = [diagnostics.CommandResult("error", b"suppressed", 1)] + [
            diagnostics.CommandResult("error") for _ in range(6)
        ]
        sample, _, all_unknown = diagnostics.sample_once(FakeExecutor(results), "adb", "emulator-5554", 0, [])
        self.assertEqual("unknown", sample["device"])
        self.assertEqual("unknown", sample["package_service"])
        self.assertEqual("unknown", sample["activity_service"])
        self.assertEqual("unknown", sample["input_service"])
        self.assertEqual("unknown", sample["collection_status"])
        self.assertTrue(all_unknown)

    def test_timeout_and_malformed_outputs_are_unknown(self):
        results = [
            diagnostics.CommandResult("timeout"),
            diagnostics.CommandResult("timeout"),
            diagnostics.CommandResult("ok", b"unexpected"),
            diagnostics.CommandResult("ok", b"unexpected"),
            diagnostics.CommandResult("ok", b"unexpected"),
            diagnostics.CommandResult("ok", b"not-a-pid"),
            diagnostics.CommandResult("ok", b"not a logcat record"),
        ]
        sample, _, all_unknown = diagnostics.sample_once(FakeExecutor(results), "adb", "emulator-5554", 0, [])
        for name in ("device", "boot_completed", "package_service", "activity_service", "input_service"):
            self.assertEqual("unknown", sample[name])
        self.assertEqual("unparsed", sample["crashes"]["status"])
        self.assertEqual("unknown", sample["collection_status"])
        self.assertTrue(all_unknown)

    def test_service_loss_and_process_restart_are_distinct(self):
        results = healthy_results()
        results[2] = diagnostics.CommandResult("ok", b"Service package: not found", 0)
        results[5] = diagnostics.CommandResult("ok", b"1800", 0)
        sample, pids, all_unknown = diagnostics.sample_once(FakeExecutor(results), "adb", "emulator-5554", 1, [1200])
        self.assertEqual("unavailable", sample["package_service"])
        self.assertEqual([1800], pids)
        self.assertTrue(sample["system_server_pid_changed"])
        self.assertFalse(all_unknown)

    def test_realistic_system_and_crash_buffer_is_parsed(self):
        categories = diagnostics._crash_categories(REAL_THREADTIME, [1200])
        self.assertEqual("observed", categories["status"])
        self.assertEqual(3, categories["total_lines"])
        self.assertEqual(3, categories["recognized_lines"])
        self.assertEqual(2, categories["system_server_lines"])
        self.assertEqual(1, categories["categories"]["system_server_fatal"])
        self.assertEqual(1, categories["categories"]["system_server_crash_hint"])
        self.assertEqual(1, categories["categories"]["system_server_oom_hint"])
        self.assertNotIn("TOPSECRET-VALUE", json.dumps(categories))

    def test_unparsed_nonempty_log_window_is_not_confident_zero(self):
        categories = diagnostics._crash_categories(b"malformed PRIVATE-RAW\n", [1200])
        self.assertEqual("unparsed", categories["status"])
        self.assertEqual("unrecognized_format", categories["diagnostic"])
        self.assertEqual(0, categories["recognized_lines"])
        self.assertTrue(categories["uncertainty"])
        self.assertNotIn("PRIVATE-RAW", json.dumps(categories))

    def test_pid_unknown_does_not_overwrite_last_observation_or_invent_change(self):
        first, first_pids, _ = diagnostics.sample_once(
            FakeExecutor(healthy_results()), "adb", "emulator-5554", 0, None
        )
        results = healthy_results()
        results[5] = diagnostics.CommandResult("timeout")
        second, second_pids, _ = diagnostics.sample_once(
            FakeExecutor(results), "adb", "emulator-5554", 5, first_pids
        )
        third, _, _ = diagnostics.sample_once(
            FakeExecutor(healthy_results()), "adb", "emulator-5554", 10, second_pids
        )
        self.assertEqual("observed", first["system_server_pid_status"])
        self.assertEqual([1200], first_pids)
        self.assertEqual("unknown", second["system_server_pid_status"])
        self.assertEqual([], second["system_server_pids"])
        self.assertEqual([1200], second_pids)
        self.assertFalse(second["system_server_pid_changed"])
        self.assertFalse(third["system_server_pid_changed"])
        self.assertEqual(0, diagnostics._summarize_samples([first, second, third])["system_server_pid_changes"])

    def test_observed_absence_and_different_pid_are_distinct_changes(self):
        first, first_pids, _ = diagnostics.sample_once(
            FakeExecutor(healthy_results()), "adb", "emulator-5554", 0, None
        )
        absent_results = healthy_results()
        absent_results[5] = diagnostics.CommandResult("ok", b"", 0)
        second, second_pids, _ = diagnostics.sample_once(
            FakeExecutor(absent_results), "adb", "emulator-5554", 5, first_pids
        )
        changed_results = healthy_results()
        changed_results[5] = diagnostics.CommandResult("ok", b"1800", 0)
        third, _, _ = diagnostics.sample_once(
            FakeExecutor(changed_results), "adb", "emulator-5554", 10, second_pids
        )
        self.assertEqual("observed", second["system_server_pid_status"])
        self.assertEqual([], second["system_server_pids"])
        self.assertTrue(second["system_server_pid_changed"])
        self.assertEqual([], second_pids)
        self.assertEqual([1800], third["system_server_pids"])
        self.assertTrue(third["system_server_pid_changed"])
        self.assertEqual(2, diagnostics._summarize_samples([first, second, third])["system_server_pid_changes"])

    def test_repeated_tail_is_not_inflated_across_samples(self):
        first, _, _ = diagnostics.sample_once(FakeExecutor(healthy_results()), "adb", "emulator-5554", 0, [1200])
        second, _, _ = diagnostics.sample_once(FakeExecutor(healthy_results()), "adb", "emulator-5554", 5, [1200])
        summary = diagnostics._summarize_samples([first, second])
        self.assertEqual(1, summary["crash_category_peak_observations"]["system_server_fatal"])
        self.assertEqual("maximum_per_bounded_window_noninflating", summary["aggregation"])

    def test_tampered_sample_records_cannot_introduce_public_fields(self):
        raw = json.dumps({
            "record_type": "sample", "elapsed_seconds": 0,
            "timestamp_utc": "2026-01-01T00:00:00Z", "device": "SECRET",
            "boot_completed": "unknown", "package_service": "unknown",
            "activity_service": "unknown", "input_service": "unknown",
            "system_server_pids": [], "system_server_pid_changed": False,
            "system_server_pid_field_count_changed": 0, "crashes": {},
            "collection_status": "observed", "unknown_command_count": 0,
            "sensitive": "TOPSECRET-VALUE",
        }).encode("utf-8")
        with tempfile.NamedTemporaryFile(mode="wb", suffix=".jsonl") as source:
            source.write(raw)
            source.flush()
            samples, _, rejected = diagnostics._parse_monitor_output(Path(source.name))
        self.assertEqual([], samples)
        self.assertEqual(1, rejected["malformed_records"])

    def test_monitor_json_is_bounded_before_parse(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "large.jsonl"
            with path.open("wb") as handle:
                handle.write(b"x" * (diagnostics.MAX_OUTPUT_BYTES + 1))
            samples, completion, rejected = diagnostics._parse_monitor_output(path)
            self.assertEqual([], samples)
            self.assertIsNone(completion)
            self.assertEqual(1, rejected["oversize_records"])


def _namespace(**overrides):
    values = {
        "ownership": "ownership.json",
        "output": "monitor.jsonl",
        "adb": "adb",
        "interval": 5,
        "lifetime": 1500,
        "command_timeout": 3,
        "phone_outcome": "success",
    }
    values.update(overrides)
    return argparse.Namespace(**values)


class StrictMetadataParsingTest(unittest.TestCase):
    def test_enum_wrong_types_return_fixed_default(self):
        self.assertEqual("fixed", diagnostics._enum(["secret"], {"valid"}, "fixed"))
        self.assertEqual("fixed", diagnostics._enum({"key": "secret"}, {"valid"}, "fixed"))

    def test_sample_elapsed_large_integer_is_bounded_without_overflow(self):
        sample, _, _ = diagnostics.sample_once(FakeExecutor(healthy_results()), "adb", "emulator-5554", 0, None)
        sample["elapsed_seconds"] = 10**400
        self.assertIsNone(diagnostics._sanitize_sample(sample))

    def test_completion_enum_wrong_types_use_fixed_sanitized_values(self):
        raw = json.dumps({
            "record_type": "completion", "sample_count": 1,
            "completion_reason": ["TOPSECRET"], "collection_failure_reason": {"value": "TOPSECRET"},
            "output_truncated": True,
        }).encode("utf-8")
        with tempfile.NamedTemporaryFile(mode="wb", suffix=".jsonl") as source:
            source.write(raw)
            source.flush()
            samples, completion, rejected = diagnostics._parse_monitor_output(Path(source.name))
        self.assertEqual([], samples)
        self.assertEqual(0, rejected["malformed_records"])
        self.assertEqual("collection_failure", completion["completion_reason"])
        self.assertEqual("all_samples_unknown", completion["collection_failure_reason"])
        self.assertNotIn("TOPSECRET", json.dumps(completion))

    def test_no_success_completion_reasons_are_preserved(self):
        for expected in ("all_samples_unknown", "no_successful_measurements"):
            with self.subTest(reason=expected):
                raw = json.dumps({
                    "record_type": "completion", "sample_count": 1,
                    "completion_reason": "deadline",
                    "collection_failure_reason": expected,
                    "output_truncated": False,
                }).encode("utf-8")
                with tempfile.NamedTemporaryFile(mode="wb", suffix=".jsonl") as source:
                    source.write(raw)
                    source.flush()
                    _, completion, rejected = diagnostics._parse_monitor_output(Path(source.name))
                self.assertEqual(0, rejected["malformed_records"])
                self.assertEqual(expected, completion["collection_failure_reason"])

    def _valid_ownership(self, directory):
        return {
            "schema": diagnostics.OWNERSHIP_SCHEMA,
            "pid": 100,
            "process_start_token": "42",
            "expected_script_sha256": "a" * 64,
            "output": str(Path(directory) / "monitor.jsonl"),
            "interval_seconds": 5,
            "lifetime_seconds": 1500,
            "command_timeout_seconds": 3,
            "phase": "pre-tablet-emulator",
            "phone_outcome": "success",
            "created_utc": "2026-01-01T00:00:00Z",
        }

    def test_ownership_phone_outcome_wrong_types_are_isolated_and_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            for wrong_value in ([], {}):
                with self.subTest(type=type(wrong_value).__name__):
                    path = Path(directory) / "ownership.json"
                    record = self._valid_ownership(directory)
                    record["phone_outcome"] = wrong_value
                    path.write_text(json.dumps(record), encoding="utf-8")
                    self.assertIsNone(diagnostics._load_ownership(path))

    def test_ownership_input_is_bounded_before_json_decode(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ownership.json"
            path.write_bytes(b"x" * (diagnostics.MAX_OUTPUT_BYTES + 1))
            self.assertIsNone(diagnostics._load_ownership(path))


class CoverageReportingTest(unittest.TestCase):
    def _observed_sample(self):
        sample, _, _ = diagnostics.sample_once(
            FakeExecutor(healthy_results()), "adb", "emulator-5554", 0, None
        )
        return sample

    def _build(self, payload, *, output_truncated=False):
        with tempfile.TemporaryDirectory() as directory:
            monitor = Path(directory) / "monitor.jsonl"
            monitor.write_text(payload, encoding="utf-8")
            ownership = {
                "phase": "pre-tablet-emulator", "phone_outcome": "success",
                "interval_seconds": 5, "lifetime_seconds": 1500,
                "command_timeout_seconds": 3,
            }
            return diagnostics._build_final_result(ownership, monitor, "failure")

    @staticmethod
    def _completion(output_truncated=False):
        return json.dumps({
            "record_type": "completion", "completion_reason": "requested_stop",
            "sample_count": 1, "output_truncated": output_truncated,
            "collection_failure_reason": "none",
        }, separators=(",", ":"))

    def test_requested_stop_with_observed_samples_is_complete(self):
        payload = json.dumps(self._observed_sample(), separators=(",", ":")) + "\n" + self._completion() + "\n"
        result, failed = self._build(payload)
        self.assertFalse(failed)
        self.assertTrue(result["coverage"]["coverage_complete"])
        self.assertEqual("none", result["coverage"]["gap_reason"])

    def test_all_unknown_samples_cannot_claim_complete_coverage(self):
        results = [diagnostics.CommandResult("error") for _ in range(7)]
        sample, _, _ = diagnostics.sample_once(FakeExecutor(results), "adb", "emulator-5554", 0, None)
        payload = json.dumps(sample, separators=(",", ":")) + "\n" + self._completion() + "\n"
        result, failed = self._build(payload)
        self.assertTrue(failed)
        self.assertFalse(result["coverage"]["coverage_complete"])
        self.assertEqual("partial_observation", result["coverage"]["gap_reason"])

    def test_rejected_records_qualify_coverage(self):
        payload = json.dumps(self._observed_sample(), separators=(",", ":")) + "\nnot-json\n" + self._completion() + "\n"
        result, failed = self._build(payload)
        self.assertFalse(failed)
        self.assertFalse(result["coverage"]["coverage_complete"])
        self.assertEqual("output_records_rejected", result["coverage"]["gap_reason"])
        self.assertEqual(1, result["output_record_rejections"]["malformed_records"])

    def test_truncated_output_cannot_claim_complete_coverage(self):
        payload = json.dumps(self._observed_sample(), separators=(",", ":")) + "\n" + self._completion(True) + "\n"
        result, failed = self._build(payload)
        self.assertTrue(failed)
        self.assertEqual("output_bound", result["collection_failure_reason"])
        self.assertFalse(result["coverage"]["coverage_complete"])
        self.assertEqual("output_truncated", result["coverage"]["gap_reason"])


def _fake_adb(directory: Path, broken: bool = False, slow: bool = False) -> Path:
    executable = directory / "adb-fixture"
    if broken:
        interpreter = "/bin/sh"
        body = "sleep 0.05\nexit 72\n"
    elif slow:
        interpreter = "/usr/bin/env python3"
        child_pid_path = directory / "adb-child.pid"
        body = f"""import os
import time
with open({json.dumps(str(child_pid_path))}, \"w\") as handle:
    handle.write(str(os.getpid()))
time.sleep(0.5)
"""
    else:
        interpreter = "/bin/sh"
        body = textwrap.dedent(
            """\
            case "$*" in
              *get-state) printf device ;;
              *getprop*) printf 1 ;;
              *service*check*package*) printf 'Service package: found' ;;
              *service*check*activity*) printf 'Service activity: found' ;;
              *service*check*input*) printf 'Service input: found' ;;
              *pidof*) printf 1200 ;;
              *logcat*) printf '09-10 12:00:00.000  1200  1200 I system_server : bounded\\n' ;;
              *) exit 64 ;;
            esac
            """
        )
    executable.write_text(f"#!{interpreter}\n{body}\n", encoding="utf-8")
    executable.chmod(executable.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return executable


def _wait_for_file(path: Path, timeout: float = 4) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists() and path.read_bytes().strip():
            return True
        time.sleep(0.02)
    return False


def _wait_for_completion(path: Path, timeout: float = 5) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists() and b'"record_type":"completion"' in path.read_bytes():
            return
        time.sleep(0.02)
    raise AssertionError("monitor did not write a bounded completion record")


class OwnershipLifecycleTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="android-diagnostics-tests-")
        self.root = Path(self.directory.name)
        self.ownership = self.root / "ownership.json"
        self.monitor_output = self.root / "monitor.jsonl"
        self.result = self.root / "result.json"
        self.original_token = diagnostics._process_start_token
        diagnostics._process_start_token = lambda pid: "424242"
        self.addCleanup(setattr, diagnostics, "_process_start_token", self.original_token)
        self.addCleanup(self.directory.cleanup)

    def _start(self, adb, lifetime=0.35, command_timeout=0.5):
        return diagnostics.main([
            "start", "--ownership", str(self.ownership), "--output", str(self.monitor_output),
            "--adb", str(adb), "--interval", "0.02", "--lifetime", str(lifetime),
            "--command-timeout", str(command_timeout), "--phone-outcome", "success",
        ])

    def _stop(self):
        return diagnostics.main([
            "stop", "--ownership", str(self.ownership), "--output", str(self.result),
            "--tablet-outcome", "failure",
        ])

    def test_start_stop_produces_sanitized_result_and_confirms_exit(self):
        adb = _fake_adb(self.root)
        self.assertEqual(0, self._start(adb))
        metadata = json.loads(self.ownership.read_text())
        self.assertTrue(_wait_for_file(self.monitor_output))
        self.assertEqual("424242", metadata["process_start_token"])
        self.assertEqual(0, self._stop())
        self.assertTrue(diagnostics._wait_for_exit(metadata["pid"], 1))
        result = json.loads(self.result.read_text())
        self.assertEqual("completed", result["status"])
        self.assertEqual("requested_stop", result["completion_reason"])
        self.assertEqual("failure", result["tablet_outcome"])
        self.assertGreater(result["coverage"]["sample_count"], 0)
        serialized = self.result.read_text()
        self.assertNotIn("PRIVATE-MARKER", serialized)
        self.assertNotIn(str(self.root), serialized)

    def test_deadline_limits_active_batch_and_marks_gap(self):
        adb = _fake_adb(self.root, slow=True)
        self.assertEqual(0, self._start(adb, lifetime=0.15, command_timeout=1))
        metadata = json.loads(self.ownership.read_text())
        child_pid_path = self.root / "adb-child.pid"
        self.assertTrue(_wait_for_file(child_pid_path))
        child_pid = int(child_pid_path.read_text())
        _wait_for_completion(self.monitor_output)
        self.assertTrue(diagnostics._wait_for_exit(child_pid, 2))
        self.assertEqual(2, self._stop())
        result = json.loads(self.result.read_text())
        self.assertEqual("collection_failure", result["status"])
        self.assertEqual("no_successful_measurements", result["collection_failure_reason"])
        self.assertEqual("deadline", result["completion_reason"])
        self.assertFalse(result["coverage"]["coverage_complete"])
        self.assertTrue(result["coverage"]["deadline_reached_before_finalization"])
        self.assertEqual("deadline_before_action_finalization", result["coverage"]["gap_reason"])
        self.assertTrue(diagnostics._wait_for_exit(metadata["pid"], 1))

    def test_stop_cancels_hanging_child_and_confirms_process_exit(self):
        adb = _fake_adb(self.root, slow=True)
        self.assertEqual(0, self._start(adb, lifetime=5, command_timeout=2))
        metadata = json.loads(self.ownership.read_text())
        child_pid_path = self.root / "adb-child.pid"
        self.assertTrue(_wait_for_file(child_pid_path))
        child_pid = int(child_pid_path.read_text())
        self.assertIn(self._stop(), {0, 2})
        self.assertTrue(diagnostics._wait_for_exit(metadata["pid"], 2))
        self.assertTrue(diagnostics._wait_for_exit(child_pid, 2))
        result = json.loads(self.result.read_text())
        self.assertIn(result["status"], {"completed", "collection_failure"})
        self.assertNotIn("stop_timeout", result["status"])
        self.assertNotIn("process_stop_failure", result["status"])

    def test_broken_adb_is_explicit_collection_failure(self):
        adb = _fake_adb(self.root, broken=True)
        self.assertEqual(0, self._start(adb, lifetime=2, command_timeout=0.5))
        self.assertTrue(_wait_for_file(self.monitor_output))
        self.assertEqual(2, self._stop())
        result = json.loads(self.result.read_text())
        self.assertEqual("collection_failure", result["status"])
        self.assertEqual("all_samples_unknown", result["collection_failure_reason"])
        self.assertGreater(result["diagnostics"]["unknown_command_count_total"], 0)
        self.assertTrue(diagnostics._wait_for_exit(json.loads(self.ownership.read_text())["pid"], 1))

    def test_corrupt_ownership_uses_fixed_fallback_without_path_leak(self):
        self.ownership.write_text("{not json", encoding="utf-8")
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = self._stop()
        self.assertEqual(2, exit_code)
        result = json.loads(self.result.read_text())
        self.assertEqual("ownership_metadata_invalid", result["status"])
        self.assertEqual("collection_failure", result["completion_reason"])
        self.assertEqual("", stdout.getvalue())
        self.assertEqual("diagnostic finalization failed: ownership_metadata_invalid\n", stderr.getvalue())
        serialized = self.result.read_text()
        self.assertNotIn("not json", serialized)
        self.assertNotIn(str(self.root), serialized)

    def test_malicious_ownership_is_rejected_without_trace_or_metadata_copy(self):
        malicious = {
            "schema": diagnostics.OWNERSHIP_SCHEMA,
            "pid": 1,
            "process_start_token": "1",
            "expected_script_sha256": "a" * 64,
            "output": str(self.monitor_output),
            "interval_seconds": float("nan"),
            "lifetime_seconds": "malicious",
            "command_timeout_seconds": 999,
            "phase": "TOPSECRET",
            "phone_outcome": "TOPSECRET",
            "created_utc": "not-a-time",
            "extra": "TOPSECRET",
        }
        self.ownership.write_text(json.dumps(malicious), encoding="utf-8")
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = self._stop()
        self.assertEqual(2, exit_code)
        self.assertEqual("", stdout.getvalue())
        self.assertEqual("diagnostic finalization failed: ownership_metadata_invalid\n", stderr.getvalue())
        result = json.loads(self.result.read_text())
        self.assertEqual("ownership_metadata_invalid", result["status"])
        self.assertEqual("unknown", result["phase"])
        self.assertEqual("unknown", result["phone_outcome"])
        serialized = self.result.read_text()
        self.assertNotIn("TOPSECRET", serialized)
        self.assertNotIn("malicious", serialized)
        self.assertNotIn(str(self.root), serialized)

    def test_ownership_large_integer_uses_fixed_fallback_without_trace(self):
        record = {
            "schema": diagnostics.OWNERSHIP_SCHEMA,
            "pid": 100,
            "process_start_token": "42",
            "expected_script_sha256": "a" * 64,
            "output": str(self.monitor_output),
            "interval_seconds": 10**400,
            "lifetime_seconds": 1500,
            "command_timeout_seconds": 3,
            "phase": "pre-tablet-emulator",
            "phone_outcome": "success",
            "created_utc": "2026-01-01T00:00:00Z",
        }
        self.ownership.write_text(json.dumps(record), encoding="utf-8")
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = self._stop()
        self.assertEqual(2, exit_code)
        self.assertEqual("", stdout.getvalue())
        self.assertEqual("diagnostic finalization failed: ownership_metadata_invalid\n", stderr.getvalue())
        result = json.loads(self.result.read_text())
        self.assertEqual("ownership_metadata_invalid", result["status"])
        serialized = self.result.read_text()
        self.assertNotIn("0" * 32, serialized)
        self.assertNotIn(str(self.root), serialized)

    def test_identity_mismatch_does_not_signal_recorded_pid(self):
        diagnostics._write_ownership(
            self.ownership,
            pid=999999,
            process_start_token="1",
            expected_script_sha256=diagnostics._sha256_file(SCRIPT),
            output=self.monitor_output,
            interval=5,
            lifetime=1500,
            command_timeout=3,
            phone_outcome="success",
        )
        self.monitor_output.write_bytes(b'{"record_type":"sample"}\n')
        original_exists = diagnostics._process_exists
        original_identity = diagnostics._process_start_token
        original_signal = diagnostics._signal_owned
        signals = []
        diagnostics._process_exists = lambda pid: True
        diagnostics._process_start_token = lambda pid: "2"
        diagnostics._signal_owned = lambda pid, number: signals.append(number) or False
        self.addCleanup(setattr, diagnostics, "_process_exists", original_exists)
        self.addCleanup(setattr, diagnostics, "_process_start_token", original_identity)
        self.addCleanup(setattr, diagnostics, "_signal_owned", original_signal)
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = self._stop()
        self.assertEqual(2, exit_code)
        self.assertEqual([], signals)
        result = json.loads(self.result.read_text())
        self.assertEqual("process_identity_mismatch", result["status"])
        self.assertNotIn(str(self.root), self.result.read_text())

    def test_start_failure_after_spawn_cleans_only_spawned_process(self):
        spawned = mock.Mock(pid=999998, poll=lambda: None)
        original_start = diagnostics._start_detached
        original_exists = diagnostics._process_exists
        original_write = diagnostics._write_ownership
        original_stop = diagnostics._stop_owned_process
        original_sha = diagnostics._sha256_file
        stopped = []
        diagnostics._start_detached = mock.Mock(return_value=spawned)
        diagnostics._process_exists = lambda pid: True
        diagnostics._write_ownership = mock.Mock(side_effect=OSError("suppressed"))
        diagnostics._stop_owned_process = lambda pid, token: stopped.append((pid, token)) or "stopped"
        diagnostics._sha256_file = lambda path: "a" * 64
        self.addCleanup(setattr, diagnostics, "_start_detached", original_start)
        self.addCleanup(setattr, diagnostics, "_process_exists", original_exists)
        self.addCleanup(setattr, diagnostics, "_write_ownership", original_write)
        self.addCleanup(setattr, diagnostics, "_stop_owned_process", original_stop)
        self.addCleanup(setattr, diagnostics, "_sha256_file", original_sha)
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            exit_code = diagnostics._start(_namespace())
        self.assertEqual(2, exit_code)
        self.assertEqual([(999998, "424242")], stopped)
        self.assertEqual("", stdout.getvalue())


class WorkflowIntegrationTest(unittest.TestCase):
    def setUp(self):
        self.workflow = (
            Path(__file__).resolve().parents[1] / ".github/workflows/business-os-mobile-ci.yml"
        ).read_text(encoding="utf-8")

    def test_tablet_monitor_wraps_existing_action_and_upload_is_sanitized(self):
        self.assertIn("id: phone", self.workflow)
        self.assertIn("id: tablet", self.workflow)
        self.assertLess(
            self.workflow.index("Start bounded tablet framework diagnostics"),
            self.workflow.index("4:3 tablet launch smoke"),
        )
        self.assertGreater(
            self.workflow.index("Finalize bounded tablet framework diagnostics"),
            self.workflow.index("tablet is not 4:3"),
        )
        upload = self.workflow[self.workflow.index("Upload sanitized tablet framework diagnostics"):]
        self.assertIn("if: always()", upload)
        self.assertIn("retention-days: 7", upload)
        self.assertIn("if-no-files-found: error", upload)
        self.assertIn("android-tablet-framework-diagnostics.json\n", upload)
        self.assertNotIn(".jsonl", upload)
        self.assertNotIn("--package", self.workflow)
        self.assertNotIn("--activity", self.workflow)

    def test_focused_suite_runs_in_ci_with_evidence_artifact(self):
        for path in (
            "src/scripts/android-framework-diagnostics.py",
            "tests/test_android_framework_diagnostics.py",
        ):
            self.assertIn(f'- "{path}"\n', self.workflow)
        job = self.workflow.split("  framework-diagnostics-tests:", 1)[1].split("\n  portable:", 1)[0]
        self.assertIn("timeout-minutes: 5", job)
        self.assertIn("python3 -m unittest -v tests/test_android_framework_diagnostics.py", job)
        self.assertIn("android-framework-diagnostics-tests.log", job)
        upload = job.split("      - name: Upload focused framework diagnostics test log", 1)[1]
        self.assertIn("if: always()", upload)
        self.assertIn("if-no-files-found: error", upload)
        self.assertIn("retention-days: 7", upload)

    def test_existing_tablet_assertions_and_profile_are_preserved(self):
        for expected in (
            'profile: "Nexus 9"',
            "adb install -r src/apps/business-os-mobile/android/app/build/outputs/apk/debug/app-debug.apk",
            "adb shell settings put system accelerometer_rotation 0",
            "adb shell settings put system user_rotation 1",
            "tablet did not rotate to landscape",
            "tablet is not 4:3",
        ):
            self.assertIn(expected, self.workflow)
        self.assertNotIn("continue-on-error", self.workflow)


if __name__ == "__main__":
    unittest.main()
