#!/usr/bin/env python3
import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
COLLECTOR_SCRIPT = SCRIPT_DIR / "automation_collect.py"
STORE_SCRIPT = SCRIPT_DIR / "automation_store.py"


def run_json(argv: list[str], stdin_text: str | None = None) -> Any:
    completed = subprocess.run(argv, input=stdin_text, capture_output=True, text=True, check=False)
    if completed.returncode != 0:
        raise SystemExit(completed.stderr.strip() or completed.stdout.strip() or "command failed")
    stdout = completed.stdout.strip()
    return json.loads(stdout) if stdout else {}


def main() -> int:
    parser = argparse.ArgumentParser(description="Run automation-engineering collectors and persist raw captures into shared SQLite.")
    parser.add_argument("--db", required=True)
    parser.add_argument("--collector")
    parser.add_argument("--repo-root")
    parser.add_argument("--target")
    args = parser.parse_args()
    python = sys.executable or "python3"
    collect = [python, str(COLLECTOR_SCRIPT)]
    if args.collector:
        collect.extend(["run", "--collector", args.collector])
    else:
        collect.append("run-all")
    if args.repo_root:
        collect.extend(["--repo-root", args.repo_root])
    raw = run_json(collect)
    run_json([python, str(STORE_SCRIPT), "init", "--db", args.db])
    stored = run_json([python, str(STORE_SCRIPT), "store-capture", "--db", args.db, "--input", "-"] + (["--target", args.target] if args.target else []), stdin_text=json.dumps(raw))
    print(json.dumps({"skill_key": "automation_engineering", "run_id": stored.get("run_id") or raw.get("run_id"), "capture_ids": stored.get("capture_ids", []), "collector": args.collector or "run-all", "raw": raw}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
