#!/usr/bin/env python3
import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
COLLECTOR_SCRIPT = SCRIPT_DIR / "linux_collect.py"
STORE_SCRIPT = SCRIPT_DIR / "discovery_store.py"


def run_json(argv: list[str], stdin_text: str | None = None) -> Any:
    completed = subprocess.run(
        argv,
        input=stdin_text,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        message = completed.stderr.strip() or completed.stdout.strip() or "command failed"
        raise SystemExit(message)
    stdout = completed.stdout.strip()
    if not stdout:
        return {}
    return json.loads(stdout)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run raw discovery collectors and persist their captures into the SQLite discovery store."
    )
    parser.add_argument("--db", required=True)
    parser.add_argument("--repo-root")
    parser.add_argument("--collector")
    parser.add_argument("--target")
    parser.add_argument("--skill-key", default="discovery_graph")
    args = parser.parse_args()

    python = sys.executable or "python3"
    collect_argv = [python, str(COLLECTOR_SCRIPT)]
    if args.collector:
        collect_argv.extend(["run", "--collector", args.collector])
    else:
        collect_argv.append("run-all")
    if args.repo_root:
        collect_argv.extend(["--repo-root", args.repo_root])

    raw_payload = run_json(collect_argv)
    if isinstance(raw_payload, dict):
        raw_payload["skill_key"] = args.skill_key
    run_json([python, str(STORE_SCRIPT), "init", "--db", args.db])
    stored = run_json(
        [python, str(STORE_SCRIPT), "store-capture", "--db", args.db, "--input", "-"]
        + (["--target", args.target] if args.target else []),
        stdin_text=json.dumps(raw_payload),
    )

    result = {
        "run_id": stored.get("run_id") or raw_payload.get("run_id"),
        "skill_key": args.skill_key,
        "capture_ids": stored.get("capture_ids", []),
        "collector": args.collector or "run-all",
        "raw": raw_payload,
    }
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
