#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
from pathlib import Path
from typing import Any

ALLOWED_EVENTS = {
    "runbook_created": "knowledge-creation",
    "runbook_confirmed": "knowledge-validation",
    "runbook_corrected": "knowledge-correction",
    "runbook_split": "knowledge-correction",
    "skillbook_updated": "knowledge-update",
}

FORBIDDEN_METADATA_KEYS = {
    "history_report",
    "history_gaps",
    "bundle_report",
    "runbook_items",
    "candidate_count",
    "gap_count",
    "promotion_ready_count",
    "family_count",
    "item_count",
    "builder_version",
    "builder_kind",
    "top_labels",
    "candidate_only",
}

FORBIDDEN_BODY_SNIPPETS = [
    "Builder-Artefakte",
    "Kernzahlen",
    "promotion_ready_count",
    "candidate_count",
    "gap_count",
    "missing_tool_actions",
    "missing_verification",
    "missing_writeback_policy",
]


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def load_env_file(path: Path) -> dict[str, str]:
    env: dict[str, str] = {}
    if not path.exists():
        return env
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        if key:
            env[key] = value
    return env


def run_ctox(ctox_bin: str, args: list[str], env_overrides: dict[str, str] | None = None) -> dict[str, Any]:
    env = os.environ.copy()
    if env_overrides:
        env.update(env_overrides)
    completed = subprocess.run(
        [ctox_bin, *args],
        check=True,
        capture_output=True,
        text=True,
        env=env,
    )
    stdout = completed.stdout.strip()
    if not stdout:
        return {}
    return json.loads(stdout)


def validate_event(event: dict[str, Any]) -> tuple[str, dict[str, Any]]:
    event_type = str(event.get("knowledge_event") or "").strip()
    if event_type not in ALLOWED_EVENTS:
        raise SystemExit(f"unsupported knowledge_event: {event_type}")

    title = str(event.get("title") or "").strip()
    body = str(event.get("body") or "").strip()
    if not title or not body:
        raise SystemExit("title and body are required")

    for snippet in FORBIDDEN_BODY_SNIPPETS:
        if snippet in body:
            raise SystemExit(f"body leaks internal builder telemetry: {snippet}")

    metadata = dict(event.get("metadata") or {})
    for key in FORBIDDEN_METADATA_KEYS:
        if key in metadata:
            raise SystemExit(f"metadata leaks internal builder telemetry: {key}")

    label = str(metadata.get("label") or event.get("label") or "").strip()
    if not label:
        raise SystemExit("knowledge event requires metadata.label")

    if event_type.startswith("runbook_"):
        metadata["label"] = label
    if event.get("ticket_key"):
        metadata["ticket_key"] = event["ticket_key"]
    if event.get("case_id"):
        metadata["case_id"] = event["case_id"]
    metadata["knowledge_event"] = event_type
    if event.get("bundle"):
        metadata["bundle"] = event["bundle"]
    if event.get("skill"):
        metadata["skill"] = event["skill"]
    return ALLOWED_EVENTS[event_type], metadata


def main() -> None:
    parser = argparse.ArgumentParser(description="Publish a validated knowledge event as CTOX self-work.")
    parser.add_argument("--ctox-bin", default="ctox")
    parser.add_argument("--system", required=True)
    parser.add_argument("--event-json")
    parser.add_argument("--events-json")
    parser.add_argument("--env-file")
    parser.add_argument("--publish", action="store_true")
    args = parser.parse_args()

    env_overrides = load_env_file(Path(args.env_file)) if args.env_file else None
    if args.events_json:
        events = load_json(Path(args.events_json))
    elif args.event_json:
        events = [load_json(Path(args.event_json))]
    else:
        raise SystemExit("either --event-json or --events-json is required")

    items: list[dict[str, Any]] = []
    for event in events:
        kind, metadata = validate_event(event)
        command = [
            "ticket",
            "self-work-put",
            "--system",
            args.system,
            "--kind",
            kind,
            "--title",
            event["title"],
            "--body",
            event["body"],
            "--metadata-json",
            json.dumps(metadata, ensure_ascii=False),
            *(["--publish"] if args.publish else []),
        ]
        skill = str(event.get("skill") or "").strip()
        if skill:
            command.extend(["--skill", skill])
        result = run_ctox(
            args.ctox_bin,
            command,
            env_overrides=env_overrides,
        )
        items.append(result.get("item", result))

    print(json.dumps({"ok": True, "count": len(items), "items": items}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
