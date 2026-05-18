#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from copy import deepcopy
from pathlib import Path
from typing import Any


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def load_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            stripped = line.strip()
            if not stripped:
                continue
            payload = json.loads(stripped)
            if isinstance(payload, dict):
                rows.append(payload)
    return rows


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def write_jsonl(path: Path, payloads: list[dict[str, Any]]) -> None:
    with path.open("w", encoding="utf-8") as handle:
        for payload in payloads:
            handle.write(json.dumps(payload, ensure_ascii=False) + "\n")


def compact_text(text: Any) -> str:
    return " ".join(str(text or "").split()).strip()


def deterministic_chunk_text(item: dict[str, Any]) -> str:
    sources = item.get("sources") or []
    source_titles = []
    for source in sources:
        if isinstance(source, dict):
            title = compact_text(source.get("title"))
            if title:
                source_titles.append(title)
    pages = [compact_text(page) for page in item.get("pages") or [] if compact_text(page)]
    return "\n".join(
        [
            compact_text(item.get("label")),
            compact_text(item.get("title")),
            compact_text(item.get("problem_class")),
            " | ".join(compact_text(entry) for entry in item.get("trigger_phrases") or [] if compact_text(entry)),
            compact_text(item.get("earliest_blocker")),
            compact_text(item.get("expected_guidance")),
            " | ".join(compact_text(entry) for entry in item.get("escalate_when") or [] if compact_text(entry)),
            " | ".join(source_titles + pages),
        ]
    ).strip()


def merge_unique_strings(base: list[Any], extra: list[Any]) -> list[str]:
    seen: set[str] = set()
    merged: list[str] = []
    for raw in [*base, *extra]:
        text = compact_text(raw)
        if not text or text in seen:
            continue
        seen.add(text)
        merged.append(text)
    return merged


def merge_sources(base: list[Any], extra: list[Any]) -> list[dict[str, Any]]:
    merged: list[dict[str, Any]] = []
    seen: set[tuple[str, str]] = set()
    for raw in [*base, *extra]:
        if not isinstance(raw, dict):
            continue
        title = compact_text(raw.get("title"))
        path = compact_text(raw.get("path"))
        key = (title, path)
        if key in seen:
            continue
        seen.add(key)
        merged.append({k: v for k, v in raw.items() if v not in (None, "", [], {})})
    return merged


def validate_promotable(item: dict[str, Any]) -> list[str]:
    reasons: list[str] = []
    if not compact_text(item.get("label")):
        reasons.append("missing_label")
    if not item.get("tool_actions"):
        reasons.append("missing_tool_actions")
    if not item.get("verification"):
        reasons.append("missing_verification")
    if not item.get("writeback_policy"):
        reasons.append("missing_writeback_policy")
    if not item.get("sources"):
        reasons.append("insufficient_source_evidence")
    if not deterministic_chunk_text(item):
        reasons.append("ambiguous_boundary")
    return sorted(set(reasons))


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Enrich a history-derived desk bundle with explicit execution supplements."
    )
    parser.add_argument("--bundle-dir", required=True)
    parser.add_argument("--supplements", required=True)
    parser.add_argument("--output-dir", required=True)
    args = parser.parse_args()

    bundle_dir = Path(args.bundle_dir)
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    main_skill = load_json(bundle_dir / "main_skill.json")
    skillbook = load_json(bundle_dir / "skillbook.json")
    runbook = load_json(bundle_dir / "runbook.json")
    items = load_jsonl(bundle_dir / "runbook_items.jsonl")
    base_report = load_json(bundle_dir / "build_report.json")
    supplements = load_jsonl(Path(args.supplements))

    supplements_by_label = {compact_text(row.get("label")): row for row in supplements if compact_text(row.get("label"))}
    enriched_items: list[dict[str, Any]] = []
    unresolved_gaps: list[dict[str, Any]] = []
    applied_supplements: list[dict[str, Any]] = []

    for item in items:
        enriched = deepcopy(item)
        label = compact_text(item.get("label"))
        supplement = supplements_by_label.get(label)
        if supplement:
            execution_guidance = compact_text(supplement.get("execution_guidance"))
            if execution_guidance:
                enriched["expected_guidance"] = compact_text(
                    f"{compact_text(enriched.get('expected_guidance'))} {execution_guidance}"
                )
            if compact_text(supplement.get("title")):
                enriched["title"] = compact_text(supplement.get("title"))
            enriched["trigger_phrases"] = merge_unique_strings(
                enriched.get("trigger_phrases") or [],
                supplement.get("trigger_phrases") or [],
            )
            enriched["entry_conditions"] = merge_unique_strings(
                enriched.get("entry_conditions") or [],
                supplement.get("entry_conditions") or [],
            )
            if compact_text(supplement.get("earliest_blocker")):
                enriched["earliest_blocker"] = compact_text(supplement.get("earliest_blocker"))
            enriched["tool_actions"] = supplement.get("tool_actions") or enriched.get("tool_actions") or []
            enriched["verification"] = merge_unique_strings(
                enriched.get("verification") or [],
                supplement.get("verification") or [],
            )
            if supplement.get("writeback_policy"):
                enriched["writeback_policy"] = supplement.get("writeback_policy")
            enriched["escalate_when"] = merge_unique_strings(
                enriched.get("escalate_when") or [],
                supplement.get("escalate_when") or [],
            )
            enriched["sources"] = merge_sources(
                enriched.get("sources") or [],
                supplement.get("sources") or [],
            )
            enriched["pages"] = merge_unique_strings(
                enriched.get("pages") or [],
                supplement.get("pages") or [],
            )
            applied_supplements.append(
                {
                    "label": label,
                    "promote_requested": bool(supplement.get("promote")),
                    "source_count": len(supplement.get("sources") or []),
                }
            )

        enriched["chunk_text"] = deterministic_chunk_text(enriched)
        reasons = validate_promotable(enriched)
        promote_requested = bool((supplement or {}).get("promote"))
        if supplement and promote_requested and not reasons:
            enriched["status"] = "active"
            enriched["gap_types"] = []
        else:
            enriched["status"] = "candidate"
            existing_gaps = item.get("gap_types") or []
            enriched["gap_types"] = merge_unique_strings(existing_gaps, reasons)
            if supplement and promote_requested and reasons:
                for reason in reasons:
                    unresolved_gaps.append(
                        {
                            "gap_id": f"{label}::enrichment::{reason}",
                            "gap_type": reason,
                            "summary": f"{label} still failed promotion after enrichment: {reason}",
                            "affected_source_refs": [label],
                            "proposed_resolution": "complete the supplement before promotion",
                            "status": "open",
                        }
                    )
        enriched_items.append(enriched)

    promoted_count = sum(1 for item in enriched_items if item.get("status") == "active")
    runbook["status"] = "active" if promoted_count > 0 else runbook.get("status", "candidate")
    runbook["item_labels"] = [item["label"] for item in enriched_items]
    if promoted_count > 0:
        skillbook["version"] = compact_text(f"{skillbook.get('version', 'v1')}-enriched")
        main_skill["writeback_flow"] = merge_unique_strings(
            main_skill.get("writeback_flow") or [],
            ["Use the promoted runbook item execution contract only after item-level verification passes."],
        )

    build_report = {
        "builder_version": "v1-history-enrichment",
        "base_bundle_dir": str(bundle_dir),
        "supplements_path": str(Path(args.supplements)),
        "item_count": len(enriched_items),
        "supplement_count": len(supplements),
        "supplements_applied": applied_supplements,
        "promotion_ready_count": promoted_count,
        "candidate_count": sum(1 for item in enriched_items if item.get("status") != "active"),
        "unresolved_gaps": unresolved_gaps,
        "summary": "History-derived desk candidates were enriched with explicit execution supplements. Only explicitly completed items are promotion-ready.",
        "base_report": base_report,
    }

    write_json(output_dir / "main_skill.json", main_skill)
    write_json(output_dir / "skillbook.json", skillbook)
    write_json(output_dir / "runbook.json", runbook)
    write_jsonl(output_dir / "runbook_items.jsonl", enriched_items)
    write_json(output_dir / "build_report.json", build_report)

    print(json.dumps(build_report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
