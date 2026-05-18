#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
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


def family_to_item(system: str, family: dict[str, Any], gaps_by_label: dict[str, list[dict[str, Any]]], skillbook_id: str, runbook_id: str) -> dict[str, Any]:
    label = family["label"]
    top_groups = family.get("top_groups", {})
    top_states = family.get("top_states", {})
    top_tokens = family.get("top_tokens", [])
    open_gap_types = [gap["gap_type"] for gap in gaps_by_label.get(label, [])]
    ticket_count = family.get("ticket_count", 0)
    title = family.get("title", "")
    guidance_parts = [
        f"Wiederkehrende Desk-Familie im {system}-Bestand mit {ticket_count} ähnlichen Fällen.",
    ]
    if top_groups:
        guidance_parts.append(
            "Bisher sichtbar vor allem in: "
            + ", ".join(f"{name} ({count})" for name, count in top_groups.items())
            + "."
        )
    if top_states:
        guidance_parts.append(
            "Typische Ticketzustände: "
            + ", ".join(f"{name} ({count})" for name, count in top_states.items())
            + "."
        )
    if open_gap_types:
        guidance_parts.append(
            "Noch offen fuer belastbare Abarbeitung: " + ", ".join(sorted(set(open_gap_types))) + "."
        )
    return {
        "item_id": f"{runbook_id}.{label.lower()}",
        "runbook_id": runbook_id,
        "skillbook_id": skillbook_id,
        "label": label,
        "title": title,
        "problem_class": f"desk.{system}.{label.lower()}",
        "trigger_phrases": top_tokens,
        "entry_conditions": [],
        "earliest_blocker": "Historischer Ticketbestand zeigt nur Desk-Koordination, nicht die eigentliche Execution.",
        "expected_guidance": " ".join(guidance_parts),
        "tool_actions": [
            {
                "tool": "ticket.source-skill-resolve",
                "mode": "desk_only",
                "target": system,
            },
            {
                "tool": "ticket.source-skill-review-note",
                "mode": "desk_only",
                "target": system,
            },
        ],
        "verification": [
            "Desk note stays grounded in the ticket text and the mirrored history.",
            "No execution claim is made from ticket history alone.",
        ],
        "writeback_policy": {
            "channel": "internal_note",
            "default_mode": "suggestion",
        },
        "escalate_when": [
            "execution detail is required but not covered by ticket history",
            "no stable desk conclusion can be justified from the mirrored records alone",
        ],
        "sources": [
            {
                "title": f"{system} ticket history",
                "path": f"{system}:history-export",
            }
        ],
        "pages": [],
        "chunk_text": "\n".join(
            [
                label,
                title,
                f"desk.{system}.{label.lower()}",
                " | ".join(top_tokens),
                "Historischer Ticketbestand zeigt nur Desk-Koordination, nicht die eigentliche Execution.",
                " ".join(guidance_parts),
                " | ".join([
                    "execution detail is required but not covered by ticket history",
                    "no stable desk conclusion can be justified from the mirrored records alone",
                ]),
                f"{system} ticket history",
            ]
        ).strip(),
        "status": "candidate",
        "gap_types": sorted(set(open_gap_types)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Build a desk-oriented candidate skillbook/runbook bundle from extracted ticket history."
    )
    parser.add_argument("--system", required=True)
    parser.add_argument("--history-report", required=True)
    parser.add_argument("--history-gaps", required=True)
    parser.add_argument("--main-skill-id", required=True)
    parser.add_argument("--skillbook-id", required=True)
    parser.add_argument("--runbook-id", required=True)
    parser.add_argument("--output-dir", required=True)
    args = parser.parse_args()

    report = load_json(Path(args.history_report))
    gaps = load_json(Path(args.history_gaps))
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    gaps_by_label: dict[str, list[dict[str, Any]]] = {}
    for gap in gaps:
        gap_id = str(gap.get("gap_id", ""))
        if "::" in gap_id:
            label = gap_id.split("::", 1)[0]
            gaps_by_label.setdefault(label, []).append(gap)
        refs = gap.get("affected_source_refs") or []
        for ref in refs:
            if isinstance(ref, dict):
                continue
            if isinstance(ref, str):
                gaps_by_label.setdefault(ref, []).append(gap)

    families = report.get("families", [])
    items = [
        family_to_item(args.system, family, gaps_by_label, args.skillbook_id, args.runbook_id)
        for family in families
    ]

    main_skill = {
        "main_skill_id": args.main_skill_id,
        "title": f"{args.system} desk main skill",
        "primary_channel": "ticket",
        "entry_action": "resolve_runbook_item",
        "resolver_contract": {"mode": "runbook-item", "retrieval_unit": "runbook_item"},
        "execution_contract": {
            "mode": "desk_note_only",
            "target": "internal_ticket_note",
            "required_output_fields": ["matched_label", "desk_summary", "boundaries"],
        },
        "resolve_flow": [
            "retrieve the best matching desk history item",
            "load parent skillbook and runbook",
            "prepare desk guidance without claiming execution knowledge",
        ],
        "writeback_flow": [
            "prefer suggestion and review for internal desk notes",
            "do not claim completion from ticket history alone",
        ],
        "linked_skillbooks": [args.skillbook_id],
        "linked_runbooks": [args.runbook_id],
    }

    skillbook = {
        "skillbook_id": args.skillbook_id,
        "title": f"{args.system} desk coordination skillbook",
        "version": "v1-candidate",
        "mission": f"Support ticket work in {args.system} by reusing mirrored desk patterns without inventing execution detail.",
        "non_negotiable_rules": [
            "Only claim desk knowledge that is visible in mirrored ticket history.",
            "Do not infer execution steps from ticket history alone.",
            "Prefer internal desk guidance over public claims when evidence is thin.",
            "Escalate whenever execution knowledge is required.",
        ],
        "runtime_policy": "Resolve the closest desk candidate, keep the note grounded in the mirrored case, and stop at the desk/execution boundary.",
        "answer_contract": "Produce concise internal desk guidance with a clear boundary when execution is not covered.",
        "workflow_backbone": [
            "identify the recurring desk family",
            "state the visible desk pattern",
            "state the execution boundary",
        ],
        "routing_taxonomy": [family["label"] for family in families],
        "linked_runbooks": [args.runbook_id],
    }

    runbook = {
        "runbook_id": args.runbook_id,
        "skillbook_id": args.skillbook_id,
        "title": f"{args.system} desk candidate families",
        "version": "v1-candidate",
        "status": "candidate",
        "problem_domain": f"{args.system}.desk",
        "item_labels": [item["label"] for item in items],
    }

    build_report = {
        "builder_version": "v1-ticket-history-desk-bundle",
        "system": args.system,
        "source_history_report": args.history_report,
        "source_gap_report": args.history_gaps,
        "item_count": len(items),
        "candidate_only": True,
        "promotion_ready_count": 0,
        "summary": "Desk bundle built from ticket history candidates. Items stay candidate-only until execution knowledge is added.",
    }

    write_json(output_dir / "main_skill.json", main_skill)
    write_json(output_dir / "skillbook.json", skillbook)
    write_json(output_dir / "runbook.json", runbook)
    write_jsonl(output_dir / "runbook_items.jsonl", items)
    write_json(output_dir / "build_report.json", build_report)

    print(json.dumps(build_report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
