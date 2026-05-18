#!/usr/bin/env python3
"""Persist an agent-curated Business OS research run payload.

This script is intentionally not a scorer. The harness agent must read,
verify, select, and score sources before calling it. The script only validates
the payload shape enough to prevent raw discovery metadata from becoming
visible Business OS results.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any


def psql_json(database_url: str, sql_text: str) -> str:
    proc = subprocess.run(
        ["psql", database_url, "--tuples-only", "--no-align", "-v", "ON_ERROR_STOP=1", "-c", sql_text],
        check=True,
        text=True,
        capture_output=True,
    )
    return proc.stdout.strip()


def ensure_store(database_url: str) -> None:
    psql_json(
        database_url,
        """
CREATE TABLE IF NOT EXISTS business_runtime_stores (
  store_key text PRIMARY KEY,
  payload_json text NOT NULL DEFAULT '{}',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);
""",
    )


def load_runs(database_url: str, store_key: str) -> list[dict[str, Any]]:
    ensure_store(database_url)
    payload = psql_json(database_url, f"SELECT payload_json FROM business_runtime_stores WHERE store_key = '{store_key}' LIMIT 1;")
    if not payload:
        return []
    value = json.loads(payload)
    return value if isinstance(value, list) else []


def save_runs(database_url: str, store_key: str, runs: list[dict[str, Any]]) -> None:
    ensure_store(database_url)
    payload = json.dumps(runs, ensure_ascii=False)
    marker = "ctox_business_research_json"
    while marker in payload:
        marker += "_x"
    sql_text = f"""
INSERT INTO business_runtime_stores (store_key, payload_json, updated_at)
VALUES ('{store_key}', ${marker}${payload}${marker}$, now())
ON CONFLICT (store_key)
DO UPDATE SET payload_json = EXCLUDED.payload_json, updated_at = now();
"""
    with tempfile.NamedTemporaryFile("w", suffix=".sql", delete=False, encoding="utf-8") as handle:
        handle.write(sql_text)
        temp_path = handle.name
    try:
        subprocess.run(["psql", database_url, "-v", "ON_ERROR_STOP=1", "-f", temp_path], check=True, text=True, capture_output=True)
    finally:
        try:
            os.unlink(temp_path)
        except OSError:
            pass


def require_string(source: dict[str, Any], key: str) -> str:
    value = source.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"source_missing_{key}: {source.get('title') or source.get('id') or '<unknown>'}")
    return value.strip()


def validate_source(source: dict[str, Any]) -> None:
    require_string(source, "id")
    require_string(source, "title")
    require_string(source, "type")
    require_string(source, "publisher")
    require_string(source, "contribution")
    require_string(source, "access")
    require_string(source, "use")
    require_string(source, "missing")
    score = source.get("score")
    if score not in {"A", "B", "C", "D"}:
        raise ValueError(f"source_invalid_score: {source.get('title') or source.get('id')}")
    score_value = source.get("scoreValue")
    if not isinstance(score_value, int) or score_value < 0 or score_value > 100:
        raise ValueError(f"source_invalid_score_value: {source.get('title') or source.get('id')}")
    if source.get("metadata_only") is True or source.get("source_type") == "paper_metadata":
        raise ValueError(f"raw_metadata_source_not_allowed: {source.get('title') or source.get('id')}")


def validate_payload(payload: dict[str, Any]) -> None:
    sources = payload.get("sources")
    graph = payload.get("graph")
    if not isinstance(sources, list):
        raise ValueError("payload_sources_must_be_array")
    if not isinstance(graph, dict) or not isinstance(graph.get("nodes"), list) or not isinstance(graph.get("edges"), list):
        raise ValueError("payload_graph_must_have_nodes_edges")
    for source in sources:
        if not isinstance(source, dict):
            raise ValueError("payload_source_must_be_object")
        validate_source(source)


def merge_run(existing: dict[str, Any], payload: dict[str, Any], run_id: str) -> dict[str, Any]:
    now_date = time.strftime("%Y-%m-%d")
    reviewed_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    sources = [{**source, "reviewedByAgent": True, "reviewedAt": reviewed_at} for source in payload["sources"]]
    screened_count = int(payload.get("screenedCount") or existing.get("screenedCount") or len(sources))
    accepted_count = int(payload.get("acceptedCount") or len(sources))
    query_count = int(payload.get("queryCount") or existing.get("queryCount") or 0)
    return {
        **existing,
        **payload,
        "id": run_id,
        "status": payload.get("status") or "synthesized",
        "updated": payload.get("updated") or now_date,
        "agentReviewedAt": reviewed_at,
        "agentReviewMode": "ctox-agent",
        "sources": sources,
        "queryCount": query_count,
        "screenedCount": screened_count,
        "acceptedCount": accepted_count,
        "researchProgress": {
            "status": "done",
            "currentStep": "Recherche aktualisiert",
            "currentQuery": "",
            "targetAdditionalSources": 0,
            "identifiedDelta": screened_count,
            "readDelta": accepted_count,
            "usedDelta": len(sources),
            "updatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--database-url", default=os.environ.get("DATABASE_URL", ""))
    parser.add_argument("--store-key", default="marketing/research/runs")
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--payload-json", required=True)
    args = parser.parse_args()

    if not args.database_url:
        raise SystemExit("DATABASE_URL required")

    payload_path = Path(args.payload_json)
    payload = json.loads(payload_path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise SystemExit("payload must be a JSON object")
    validate_payload(payload)

    runs = load_runs(args.database_url, args.store_key)
    existing = next((item for item in runs if item.get("id") == args.run_id), {})
    next_run = merge_run(existing, payload, args.run_id)
    next_runs = [next_run, *[item for item in runs if item.get("id") != args.run_id]]
    save_runs(args.database_url, args.store_key, next_runs)
    print(json.dumps({"ok": True, "run_id": args.run_id, "sources": len(next_run.get("sources", []))}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
