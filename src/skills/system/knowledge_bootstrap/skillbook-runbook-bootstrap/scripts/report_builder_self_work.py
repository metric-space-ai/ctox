#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
from typing import Any


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def summarize_families(history_report: dict[str, Any], limit: int) -> list[str]:
    families = history_report.get("families") or []
    rows: list[str] = []
    for family in families[:limit]:
        label = str(family.get("label") or "").strip()
        title = str(family.get("title") or "").strip()
        ticket_count = int(family.get("ticket_count") or 0)
        groups = family.get("top_groups") or {}
        group_summary = ", ".join(
            f"{name} ({count})" for name, count in list(groups.items())[:2] if str(name).strip()
        )
        line = f"{label}: {title} ({ticket_count} Fälle)"
        if group_summary:
            line += f" | Gruppen: {group_summary}"
        rows.append(line)
    return rows


def summarize_gap_types(history_gaps: list[dict[str, Any]], limit: int) -> list[str]:
    counts: Counter[str] = Counter()
    for gap in history_gaps:
        gap_type = str(gap.get("gap_type") or "").strip()
        if gap_type:
            counts[gap_type] += 1
    return [f"{gap_type}: {count}" for gap_type, count in counts.most_common(limit)]


def build_body(
    system: str,
    history_report: dict[str, Any],
    bundle_report: dict[str, Any],
    family_lines: list[str],
    gap_lines: list[str],
) -> str:
    candidate_count = int(history_report.get("candidate_count") or 0)
    family_count = int(history_report.get("family_count") or 0)
    item_count = int(bundle_report.get("item_count") or 0)
    promotion_ready_count = int(bundle_report.get("promotion_ready_count") or 0)
    lines = [
        f"Aus der gespiegelten {system}-Historie wurde ein erster Desk-Bundle-Kandidat erzeugt.",
        f"Sichtbarer Stand: {family_count} wiederkehrende Familien, {candidate_count} Kandidaten, {item_count} erzeugte Runbook-Items, {promotion_ready_count} promotionsreif.",
    ]
    if family_lines:
        lines.append("Stärkste Kandidaten:")
        lines.extend(f"- {line}" for line in family_lines)
    if gap_lines:
        lines.append("Offene Builder-Lücken:")
        lines.extend(f"- {line}" for line in gap_lines)
    lines.append(
        "Aus der Historie allein ist damit derzeit nur Desk-Wissen ableitbar; Execution bleibt bis zur Anreicherung mit Manuals, Runbooks oder verifizierten Ausführungen offen."
    )
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Render an internal builder report payload. Builder telemetry must not be published."
    )
    parser.add_argument("--system", required=True)
    parser.add_argument("--history-report", required=True)
    parser.add_argument("--history-gaps", required=True)
    parser.add_argument("--bundle-report", required=True)
    parser.add_argument("--runbook-items")
    parser.add_argument("--title")
    parser.add_argument("--family-limit", type=int, default=5)
    parser.add_argument("--gap-limit", type=int, default=5)
    args = parser.parse_args()

    history_report_path = Path(args.history_report)
    history_gaps_path = Path(args.history_gaps)
    bundle_report_path = Path(args.bundle_report)
    runbook_items_path = Path(args.runbook_items) if args.runbook_items else None

    history_report = load_json(history_report_path)
    history_gaps = load_json(history_gaps_path)
    bundle_report = load_json(bundle_report_path)

    family_lines = summarize_families(history_report, args.family_limit)
    gap_lines = summarize_gap_types(history_gaps, args.gap_limit)
    title = args.title or f"CTOX: erster Desk-Bundle-Kandidat aus {args.system}-Historie erstellt"
    body = build_body(args.system, history_report, bundle_report, family_lines, gap_lines)
    metadata = {
        "builder_kind": "knowledge-build-report",
        "history_report": str(history_report_path),
        "history_gaps": str(history_gaps_path),
        "bundle_report": str(bundle_report_path),
        "runbook_items": str(runbook_items_path) if runbook_items_path else None,
        "family_count": history_report.get("family_count", 0),
        "candidate_count": history_report.get("candidate_count", 0),
        "gap_count": history_report.get("gap_count", 0),
        "item_count": bundle_report.get("item_count", 0),
        "promotion_ready_count": bundle_report.get("promotion_ready_count", 0),
        "candidate_only": bundle_report.get("candidate_only", False),
        "top_labels": [family.get("label") for family in (history_report.get("families") or [])[: args.family_limit]],
    }
    payload = {
        "system": args.system,
        "title": title,
        "body": body,
        "metadata": metadata,
        "runbook_items": str(runbook_items_path) if runbook_items_path else None,
        "publishable": False,
        "publication_decision": "DO_NOT_PUBLISH",
        "reason": "Builder telemetry contains internal process status, counts, gap summaries, and artifact references. It is not a publishable knowledge event.",
    }
    print(json.dumps(payload, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
