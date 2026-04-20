#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description="Read the simulated Printengine alert state.")
    parser.add_argument("--snapshot", required=True)
    parser.add_argument("--service", default="prestige_printengine")
    args = parser.parse_args()

    payload = json.loads(Path(args.snapshot).read_text(encoding="utf-8"))
    service = str(payload.get("service") or "").strip()
    if service != args.service:
        raise SystemExit(f"snapshot service mismatch: expected {args.service}, got {service or 'empty'}")

    state = str(payload.get("state") or "").strip().lower()
    result = {
        "ok": True,
        "host": payload.get("host"),
        "service": service,
        "state": state,
        "severity": payload.get("severity"),
        "checked_at": payload.get("checked_at"),
        "recovered": bool(payload.get("recovered")),
        "recovered_at": payload.get("recovered_at"),
        "summary": payload.get("summary"),
        "writeback_boundary": (
            "no_entwarnung_without_recovered_signal"
            if state == "critical"
            else "recovered_signal_required_before_entwarnung"
        ),
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
