#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import http.server
import json
import os
import re
import socketserver
import subprocess
import sys
import tempfile
import textwrap
import threading
import time
from pathlib import Path
from typing import Any


BENCH_ROOT = Path(__file__).resolve().parent
REPO_ROOT = BENCH_ROOT.parents[2]
CASE_DIR = BENCH_ROOT / "cases"
FIXTURE_SITE_DIR = BENCH_ROOT / "fixtures" / "site"
REQUIRED_FIXTURES = [
    "guide.html",
    "browser.html",
    "browser-form.html",
    "browser-login.html",
    "browser-dashboard.html",
    "browser-table.html",
    "browser-wizard.html",
    "browser-docs-index.html",
    "browser-docs-deploy.html",
    "browser-checklist.html",
]

PLACEHOLDER_RE = re.compile(r"\$\{([A-Z0-9_]+)\}")


class BenchError(RuntimeError):
    pass


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        return


class ThreadingHttpServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True


class FixtureServer:
    def __init__(self, directory: Path) -> None:
        self.directory = directory
        self.server: ThreadingHttpServer | None = None
        self.thread: threading.Thread | None = None
        self.base_url: str | None = None

    def start(self) -> str:
        handler = lambda *args, **kwargs: QuietHandler(  # noqa: E731
            *args,
            directory=str(self.directory),
            **kwargs,
        )
        self.server = ThreadingHttpServer(("127.0.0.1", 0), handler)
        host, port = self.server.server_address
        self.base_url = f"http://{host}:{port}"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        return self.base_url

    def stop(self) -> None:
        if self.server is not None:
            self.server.shutdown()
            self.server.server_close()
        if self.thread is not None:
            self.thread.join(timeout=2)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Standalone regression bench for the CTOX web stack."
    )
    parser.add_argument(
        "--ctox-bin",
        default=os.environ.get("CTOX_WEB_BENCH_BIN", "ctox"),
        help="Path to the built ctox binary. Defaults to CTOX_WEB_BENCH_BIN or `ctox`.",
    )
    parser.add_argument(
        "--tier",
        choices=["fixture", "live", "all"],
        default="fixture",
        help="Which case tier to run.",
    )
    parser.add_argument(
        "--case",
        action="append",
        default=[],
        help="Run only the selected case id. Can be passed multiple times.",
    )
    parser.add_argument(
        "--report",
        help="Optional path to write the JSON report.",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="List discovered cases and exit.",
    )
    parser.add_argument(
        "--validate-only",
        action="store_true",
        help="Validate manifests and fixtures without executing ctox.",
    )
    parser.add_argument(
        "--keep-tmp-root",
        action="store_true",
        help="Keep the temporary CTOX_ROOT around for debugging.",
    )
    return parser.parse_args()


def load_case_files() -> list[dict[str, Any]]:
    manifests: list[dict[str, Any]] = []
    for path in sorted(CASE_DIR.glob("*.json")):
        with path.open("r", encoding="utf-8") as handle:
            payload = json.load(handle)
        if payload.get("version") != 1:
            raise BenchError(f"{path} must declare version 1")
        shared_setup = payload.get("shared_setup", [])
        if not isinstance(shared_setup, list):
            raise BenchError(f"{path} must declare shared_setup as an array")
        cases = payload.get("cases")
        if not isinstance(cases, list):
            raise BenchError(f"{path} must contain a `cases` array")
        for case in cases:
            case["_manifest_path"] = str(path)
            case["_shared_setup"] = shared_setup
            manifests.append(case)
    return manifests


def validate_case(case: dict[str, Any]) -> None:
    required = ["id", "tier", "tool", "command", "assertions"]
    if "expect_failure" in case:
        required = ["id", "tier", "tool", "command"]
    for key in required:
        if key not in case:
            raise BenchError(f"case in {case.get('_manifest_path')} is missing `{key}`")
    if case["tier"] not in {"fixture", "live"}:
        raise BenchError(f"case {case['id']} has unsupported tier {case['tier']}")
    if not isinstance(case["command"], list) or not case["command"]:
        raise BenchError(f"case {case['id']} must provide a non-empty command array")
    if "expect_failure" in case:
        validate_expected_failure(case["id"], case["expect_failure"])
    elif not isinstance(case["assertions"], list) or not case["assertions"]:
        raise BenchError(f"case {case['id']} must provide assertions")
    for assertion in case.get("assertions", []):
        validate_assertion(case["id"], assertion)
    for step in case.get("_shared_setup", []):
        validate_step(step, case["id"], "shared_setup")
    for step in case.get("setup", []):
        validate_step(step, case["id"], "setup")
        for assertion in step.get("assertions", []):
            validate_assertion(case["id"], assertion)


def validate_step(step: dict[str, Any], case_id: str, label: str) -> None:
    if "write_json" not in step and "write_text" not in step and "command" not in step:
        raise BenchError(
            f"{label} step in {case_id} must contain write_json, write_text, or command"
        )


def validate_assertion(case_id: str, assertion: dict[str, Any]) -> None:
    if "path" not in assertion:
        raise BenchError(f"assertion in {case_id} must contain a path")
    operators = ["equals", "contains", "len_eq", "len_gte", "truthy"]
    if not any(name in assertion for name in operators):
        raise BenchError(f"assertion {assertion['path']} in {case_id} has no operator")


def validate_expected_failure(case_id: str, expected: dict[str, Any]) -> None:
    if not isinstance(expected, dict):
        raise BenchError(f"expect_failure in {case_id} must be an object")
    if "contains" not in expected:
        raise BenchError(f"expect_failure in {case_id} must define `contains`")


def substitute_placeholders(value: Any, mapping: dict[str, str]) -> Any:
    if isinstance(value, str):
        def replace(match: re.Match[str]) -> str:
            key = match.group(1)
            if key not in mapping:
                raise BenchError(f"unknown placeholder {key}")
            return mapping[key]

        return PLACEHOLDER_RE.sub(replace, value)
    if isinstance(value, list):
        return [substitute_placeholders(item, mapping) for item in value]
    if isinstance(value, dict):
        return {
            str(substitute_placeholders(key, mapping)): substitute_placeholders(item, mapping)
            for key, item in value.items()
        }
    return value


def should_run_case(case: dict[str, Any], selected_tier: str, selected_ids: set[str]) -> bool:
    if selected_tier != "all" and case["tier"] != selected_tier:
        return False
    if selected_ids and case["id"] not in selected_ids:
        return False
    return True


def resolve_json_path(payload: Any, path: str) -> Any:
    current = payload
    for raw_part in path.split("."):
        part = raw_part.strip()
        if not part:
            raise BenchError(f"invalid empty path segment in {path}")
        if isinstance(current, list):
            try:
                index = int(part)
            except ValueError as exc:
                raise BenchError(f"path {path} expects numeric index at {part}") from exc
            try:
                current = current[index]
            except IndexError as exc:
                raise BenchError(f"path {path} index {index} out of range") from exc
        elif isinstance(current, dict):
            if part not in current:
                raise BenchError(f"path {path} is missing key {part}")
            current = current[part]
        else:
            raise BenchError(f"path {path} cannot descend into non-container value")
    return current


def run_assertions(payload: Any, assertions: list[dict[str, Any]], case_id: str) -> None:
    for assertion in assertions:
        value = resolve_json_path(payload, assertion["path"])
        if "equals" in assertion:
            expected = assertion["equals"]
            if value != expected:
                raise BenchError(
                    f"{case_id}: {assertion['path']} expected {expected!r} but got {value!r}"
                )
        if "contains" in assertion:
            needle = assertion["contains"]
            haystack = value if isinstance(value, str) else json.dumps(value, sort_keys=True)
            if needle not in haystack:
                raise BenchError(
                    f"{case_id}: {assertion['path']} does not contain {needle!r}"
                )
        if "len_eq" in assertion:
            if not hasattr(value, "__len__"):
                raise BenchError(f"{case_id}: {assertion['path']} has no length")
            if len(value) != assertion["len_eq"]:
                raise BenchError(
                    f"{case_id}: {assertion['path']} expected len {assertion['len_eq']} but got {len(value)}"
                )
        if "len_gte" in assertion:
            if not hasattr(value, "__len__"):
                raise BenchError(f"{case_id}: {assertion['path']} has no length")
            if len(value) < assertion["len_gte"]:
                raise BenchError(
                    f"{case_id}: {assertion['path']} expected len >= {assertion['len_gte']} but got {len(value)}"
                )
        if "truthy" in assertion:
            if bool(value) is not assertion["truthy"]:
                raise BenchError(
                    f"{case_id}: {assertion['path']} truthiness expected {assertion['truthy']} but got {bool(value)}"
                )


def parse_json_output(raw: str, case_id: str) -> Any:
    try:
        return json.loads(raw)
    except json.JSONDecodeError as exc:
        raise BenchError(f"{case_id}: command did not return valid JSON: {exc}") from exc


def run_ctox(
    ctox_bin: str,
    command: list[str],
    env_overrides: dict[str, str],
    cwd: Path,
    _: str,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env.update(env_overrides)
    started = time.monotonic()
    completed = subprocess.run(
        [ctox_bin, *command],
        cwd=str(cwd),
        env=env,
        text=True,
        capture_output=True,
    )
    duration_ms = int((time.monotonic() - started) * 1000)
    completed.duration_ms = duration_ms  # type: ignore[attr-defined]
    return completed


def execute_ctox(
    ctox_bin: str,
    command: list[str],
    env_overrides: dict[str, str],
    cwd: Path,
    case_id: str,
) -> tuple[Any, subprocess.CompletedProcess[str]]:
    completed = run_ctox(ctox_bin, command, env_overrides, cwd, case_id)
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        raise BenchError(f"{case_id}: command failed: {detail}")
    payload = parse_json_output(completed.stdout, case_id)
    return payload, completed


def execute_setup_step(
    step: dict[str, Any],
    ctox_bin: str,
    env: dict[str, str],
    cwd: Path,
    variables: dict[str, str],
    case_id: str,
) -> None:
    if "write_json" in step:
        spec = substitute_placeholders(step["write_json"], variables)
        path = Path(spec["path"])
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("w", encoding="utf-8") as handle:
            json.dump(spec["value"], handle, indent=2, sort_keys=True)
            handle.write("\n")
        return

    if "write_text" in step:
        spec = substitute_placeholders(step["write_text"], variables)
        path = Path(spec["path"])
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(spec["content"], encoding="utf-8")
        return

    command = substitute_placeholders(step["command"], variables)
    step_env = env.copy()
    step_env.update(substitute_placeholders(step.get("env", {}), variables))
    payload, _ = execute_ctox(ctox_bin, command, step_env, cwd, case_id)
    if step.get("assertions"):
        assertions = substitute_placeholders(step["assertions"], variables)
        run_assertions(payload, assertions, case_id)


def build_report(
    results: list[dict[str, Any]],
    tier: str,
    ctox_bin: str,
) -> dict[str, Any]:
    passed = sum(1 for item in results if item["status"] == "passed")
    failed = sum(1 for item in results if item["status"] == "failed")
    return {
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "tier": tier,
        "ctox_bin": ctox_bin,
        "case_count": len(results),
        "summary": {
            "passed": passed,
            "failed": failed,
        },
        "results": results,
    }


def main() -> int:
    args = parse_args()
    cases = load_case_files()
    for case in cases:
        validate_case(case)

    selected = [case for case in cases if should_run_case(case, args.tier, set(args.case))]
    if args.list:
        for case in selected:
            print(f"{case['tier']}\t{case['id']}\t{case['tool']}")
        return 0

    if args.validate_only:
        for required in REQUIRED_FIXTURES:
            if not FIXTURE_SITE_DIR.joinpath(required).exists():
                raise BenchError(f"fixture site is incomplete: missing {required}")
        print(f"validated {len(selected)} cases")
        return 0

    if not selected:
        raise BenchError("no cases selected")

    results: list[dict[str, Any]] = []
    fixture_server: FixtureServer | None = None
    shared_root_handle: tempfile.TemporaryDirectory[str] | None = None
    try:
        shared_root_handle = tempfile.TemporaryDirectory(prefix="ctox-web-bench-shared-")
        shared_root = Path(shared_root_handle.name)
        fixture_server = FixtureServer(FIXTURE_SITE_DIR)
        fixture_base_url = fixture_server.start()
        executed_shared_setup: set[str] = set()

        for case in selected:
            case_started = time.monotonic()
            case_id = case["id"]
            case_result = {
                "id": case_id,
                "tier": case["tier"],
                "tool": case["tool"],
                "families": case.get("families", []),
                "status": "passed",
            }
            case_tmp_handle = tempfile.TemporaryDirectory(prefix="ctox-web-bench-")
            case_tmp_root = Path(case_tmp_handle.name)
            try:
                case_tmp_root.joinpath("runtime").mkdir(parents=True, exist_ok=True)
                case_variables = {
                    "BENCH_ROOT": str(BENCH_ROOT),
                    "REPO_ROOT": str(REPO_ROOT),
                    "TMP_ROOT": str(case_tmp_root),
                    "SHARED_ROOT": str(shared_root),
                    "FIXTURE_BASE_URL": fixture_base_url,
                }
                case_env = {"CTOX_ROOT": str(case_tmp_root)}
                case_env.update(
                    substitute_placeholders(case.get("env", {}), case_variables)
                )
                manifest_path = case["_manifest_path"]
                if manifest_path not in executed_shared_setup:
                    shared_variables = {
                        "BENCH_ROOT": str(BENCH_ROOT),
                        "REPO_ROOT": str(REPO_ROOT),
                        "TMP_ROOT": str(shared_root),
                        "SHARED_ROOT": str(shared_root),
                        "FIXTURE_BASE_URL": fixture_base_url,
                    }
                    shared_env = {"CTOX_ROOT": str(shared_root)}
                    for step in case.get("_shared_setup", []):
                        execute_setup_step(
                            step,
                            args.ctox_bin,
                            shared_env,
                            REPO_ROOT,
                            shared_variables,
                            case_id,
                        )
                    executed_shared_setup.add(manifest_path)
                for step in case.get("setup", []):
                    execute_setup_step(
                        step,
                        args.ctox_bin,
                        case_env,
                        REPO_ROOT,
                        case_variables,
                        case_id,
                    )

                command = substitute_placeholders(case["command"], case_variables)
                completed = run_ctox(
                    args.ctox_bin,
                    command,
                    case_env,
                    REPO_ROOT,
                    case_id,
                )
                if "expect_failure" in case:
                    expected = substitute_placeholders(
                        case["expect_failure"], case_variables
                    )
                    if completed.returncode == 0:
                        raise BenchError(
                            f"{case_id}: command succeeded but failure was expected"
                        )
                    detail = completed.stderr.strip() or completed.stdout.strip()
                    if expected["contains"] not in detail:
                        raise BenchError(
                            f"{case_id}: failure detail did not contain {expected['contains']!r}: {detail}"
                        )
                else:
                    if completed.returncode != 0:
                        detail = completed.stderr.strip() or completed.stdout.strip()
                        raise BenchError(f"{case_id}: command failed: {detail}")
                    payload = parse_json_output(completed.stdout, case_id)
                    assertions = substitute_placeholders(
                        case["assertions"], case_variables
                    )
                    run_assertions(payload, assertions, case_id)
                case_result["stdout_excerpt"] = completed.stdout.strip()[:2000]
                case_result["stderr_excerpt"] = completed.stderr.strip()[:2000]
            except Exception as exc:  # noqa: BLE001
                case_result["status"] = "failed"
                case_result["error"] = str(exc)
            case_result["duration_ms"] = int((time.monotonic() - case_started) * 1000)
            results.append(case_result)
            if args.keep_tmp_root:
                print(
                    textwrap.dedent(
                        f"""
                        kept temporary CTOX_ROOT for case {case_id}:
                        {case_tmp_root}
                        """
                    ).strip(),
                    file=sys.stderr,
                )
            else:
                case_tmp_handle.cleanup()

        report = build_report(results, args.tier, args.ctox_bin)
        if args.report:
            report_path = Path(args.report)
            report_path.parent.mkdir(parents=True, exist_ok=True)
            with report_path.open("w", encoding="utf-8") as handle:
                json.dump(report, handle, indent=2, sort_keys=True)
                handle.write("\n")

        print(json.dumps(report, indent=2, sort_keys=True))
        return 1 if report["summary"]["failed"] else 0
    finally:
        if fixture_server is not None:
            fixture_server.stop()
        if shared_root_handle is not None:
            shared_root_handle.cleanup()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BenchError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(2) from exc
