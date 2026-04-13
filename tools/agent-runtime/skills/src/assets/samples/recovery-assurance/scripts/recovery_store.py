#!/usr/bin/env python3
import argparse
import importlib.util
import json
import sys
from pathlib import Path


def load_shared_store():
    path = Path(__file__).resolve().parents[2] / "discovery-graph" / "scripts" / "discovery_store.py"
    spec = importlib.util.spec_from_file_location("shared_discovery_store", path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(module)
    return module


def main() -> int:
    parser = argparse.ArgumentParser(description="Shared SQLite persistence wrapper for recovery_assurance.")
    subparsers = parser.add_subparsers(dest="command", required=True)
    init_parser = subparsers.add_parser("init")
    init_parser.add_argument("--db", required=True)
    capture_parser = subparsers.add_parser("store-capture")
    capture_parser.add_argument("--db", required=True)
    capture_parser.add_argument("--input", required=True)
    capture_parser.add_argument("--target")
    graph_parser = subparsers.add_parser("store-graph")
    graph_parser.add_argument("--db", required=True)
    graph_parser.add_argument("--input", required=True)
    args = parser.parse_args()
    shared = load_shared_store()
    conn = shared.open_db(args.db)
    if args.command == "init":
        print(json.dumps({"ok": True, "db_path": str(Path(args.db).resolve())}, indent=2))
        return 0
    payload = shared.load_json(args.input)
    payload["skill_key"] = "recovery_assurance"
    if args.command == "store-capture":
        print(json.dumps({"ok": True, **shared.store_capture(conn, payload, args.target)}, indent=2))
        return 0
    print(json.dumps({"ok": True, **shared.store_graph(conn, payload)}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
