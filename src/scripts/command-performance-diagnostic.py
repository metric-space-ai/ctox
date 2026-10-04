#!/usr/bin/env python3
"""One historical, same-runner comparison. Never a release gate waiver."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

VARIANTS = {
    "baseline": ("20e0b8ee7febad2be4218b3bff0da9ece9bb6f36", 36564350911,
                 "6d362fcaddd528e19f5978970ddce5c60601ca21862529917cf3947344fc4ac5"),
    "current": ("c061134a8f2b163fc871b61fc66df2819138aa8d", 36606718403,
                "3ec5f32e5076fd87d2c8c96c1b69a2272e5851ccac3a99fb1f880a3651fdc34b"),
}
ORDER = ("baseline", "current", "current", "baseline")


def provenance(root, binary, proof, source, expected_digest):
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if revision != source or (proof / "source-revision.txt").read_text().strip() != source:
        raise ValueError("checkout/artifact source identity mismatch")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=root):
        raise ValueError("comparison source must be clean")
    expected = (proof / "binary.sha256").read_text().split()[0]
    hasher = hashlib.sha256()
    with binary.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(chunk)
    digest = hasher.hexdigest()
    if expected != expected_digest or digest != expected_digest:
        raise ValueError("artifact binary checksum mismatch")
    return {"source": source, "binary_sha256": digest,
            "build_profile": (proof / "build-profile.txt").read_text().strip()}


def bounded_run(command, cwd, env, log, timeout=180, byte_limit=16 * 1024 * 1024):
    """Own one process group, cap time/logs, and stop it before another probe."""
    with log.open("wb") as output:
        child = subprocess.Popen(command, cwd=cwd, env=env, stdout=output,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        deadline = time.monotonic() + timeout
        reason = None
        try:
            while child.poll() is None:
                if time.monotonic() >= deadline:
                    reason = "deadline"
                    break
                if log.stat().st_size > byte_limit:
                    reason = "log-byte-limit"
                    break
                time.sleep(0.1)
            if reason:
                try:
                    os.killpg(child.pid, signal.SIGINT)
                except ProcessLookupError:
                    pass
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    pass
        finally:
            # Also close owned descendants after cancellation or parent exit.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait(timeout=10)
        if reason:
            raise RuntimeError(f"probe stopped: {reason}; owned process group closed")
        return child.returncode


def host_snapshot():
    return {"at_ms": time.time_ns() // 1_000_000,
            "loadavg": Path("/proc/loadavg").read_text().strip(),
            "cpu_stat": Path("/proc/stat").read_text(),
            "meminfo": Path("/proc/meminfo").read_text()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in VARIANTS:
        for suffix in ("root", "binary", "proof"):
            parser.add_argument(f"--{name}-{suffix}", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not Path("/proc/stat").exists():
        raise ValueError("diagnostic requires one Linux runner")
    args.output.mkdir(parents=True, exist_ok=False)
    summary = {"schema": "ctox.command_same_runner_diagnostic.v1",
               "diagnostic_only": True, "release_acceptance": False,
               "order": ORDER, "provenance": {}, "runs": []}
    try:
        # Validate BOTH identities before starting any browser or native process.
        for name, (source, run_id, digest) in VARIANTS.items():
            summary["provenance"][name] = provenance(
                getattr(args, name + "_root"), getattr(args, name + "_binary"),
                getattr(args, name + "_proof"), source, digest)
            summary["provenance"][name]["artifact_run_id"] = run_id
            root = getattr(args, name + "_root")
            proof = getattr(args, name + "_proof")
            summary["provenance"][name]["rustc"] = (proof / "rustc-version.txt").read_text().strip()
            for file in ("Cargo.lock", "src/apps/business-os/package-lock.json"):
                summary["provenance"][name][file] = hashlib.sha256((root / file).read_bytes()).hexdigest()
        for field in ("build_profile", "rustc", "Cargo.lock", "src/apps/business-os/package-lock.json"):
            if summary["provenance"]["baseline"][field] != summary["provenance"]["current"][field]:
                raise ValueError(f"comparison build/dependency mismatch: {field}")
        summary["cpu_model"] = subprocess.check_output(["lscpu"], text=True)
        for index, name in enumerate(ORDER, 1):
            root = getattr(args, name + "_root").resolve()
            binary = getattr(args, name + "_binary").resolve()
            directory = (args.output / f"{index}-{name}").resolve()
            directory.mkdir()
            env = {**os.environ, "CTOX_BIN": str(binary), "CTOX_SKIP_SMOKE_BUILD": "1",
                   "SMOKE_MODE": "command-roundtrip-timing-browser-to-rust",
                   "SMOKE_PAGE_PATH": "/index.html",
                   "SMOKE_COMMAND_TIMING_OUTPUT": str(directory / "marks.json"),
                   "SMOKE_COMMAND_TIMING_REPORT_OUTPUT": str(directory / "stages.json"),
                   "SMOKE_PROCESS_LIFECYCLE_PATH": str(directory / "processes.json"),
                   "CARGO_BUILD_JOBS": "2", "RUST_TEST_THREADS": "2"}
            # Do not inherit a symbol capture or customer profile/root into this
            # fresh fixture; the smoke removes its owned temporary root itself.
            for key in ("CTOX_SMOKE_ROOT", "SMOKE_BROWSER_PROFILE_DIR", "CTOX_SQLITE",
                        "CTOX_SMOKE_KEEP_ARTIFACTS"):
                env.pop(key, None)
            before = host_snapshot()
            code = bounded_run(["node", "src/core/rxdb/tools/browser_rust_smoke.js"],
                               root, env, directory / "browser.log")
            after = host_snapshot()
            lifecycle = json.loads((directory / "processes.json").read_text())
            if code or not any(e["type"] == "smoke_cleanup_complete" for e in lifecycle["events"]):
                raise RuntimeError(f"{index}-{name}: smoke or cleanup failed; no further probe")
            if subprocess.check_output(["git", "status", "--porcelain"], cwd=root):
                raise RuntimeError(f"{index}-{name}: source cleanup failed; no further probe")
            stages = json.loads((directory / "stages.json").read_text())
            if stages["sample_count"] != 30 or stages["complete_count"] != 30 or stages["issues"]:
                raise RuntimeError(f"{index}-{name}: incomplete timing evidence")
            budget = bounded_run(["node", "src/apps/business-os/rxdb/tests/command-roundtrip-budget.mjs",
                                  "--input", str(directory / "marks.json")],
                                 root, env, directory / "budget.log", timeout=10)
            summary["runs"].append({"variant": name, "index": index,
                                    "budget_exit": budget, "stages": stages["summary"],
                                    "host_before": before, "host_after": after})
            print(f"{index}-{name}: p50={stages['summary']['total']['raw']['p50']} ms; budget_exit={budget}", flush=True)
        return 1 if any(r["budget_exit"] for r in summary["runs"]) else 0
    except BaseException as error:
        summary["error"] = str(error)
        raise
    finally:
        (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")


if __name__ == "__main__":
    def terminate(_signal, _frame):
        raise KeyboardInterrupt("terminated")
    signal.signal(signal.SIGTERM, terminate)
    raise SystemExit(main())
