#!/usr/bin/env python3
import argparse
import importlib.util
import json
import sys
from pathlib import Path


def load_shared_query():
    path = Path(__file__).resolve().parents[2] / "discovery-graph" / "scripts" / "discovery_query.py"
    spec = importlib.util.spec_from_file_location("shared_discovery_query", path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(module)
    return module


def main() -> int:
    parser = argparse.ArgumentParser(description="Query change_lifecycle state from the shared SQLite kernel.")
    subparsers = parser.add_subparsers(dest="command", required=True)
    summary_parser = subparsers.add_parser("summary")
    summary_parser.add_argument("--db", required=True)
    export_parser = subparsers.add_parser("export-cytoscape")
    export_parser.add_argument("--db", required=True)
    args = parser.parse_args()
    shared = load_shared_query()
    conn = shared.open_db(args.db)
    if args.command == "summary":
        print(json.dumps(shared.summary(conn, "change_lifecycle"), indent=2))
        return 0
    print(json.dumps(shared.export_cytoscape(conn, "change_lifecycle"), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
