#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


TOKEN_RE = re.compile(r"[A-Za-zÄÖÜäöüß0-9._/-]{4,}")
STOPWORDS = {
    "ticket",
    "zammad",
    "mail",
    "email",
    "note",
    "open",
    "closed",
    "normal",
    "high",
    "low",
    "problem",
    "service",
    "alert",
    "filialserver",
    "original",
    "opened",
    "source",
    "system",
    "channel",
    "category",
    "subcategory",
    "type",
    "impact",
    "branch",
    "location",
    "imported",
    "request",
    "context",
    "einzelplatz",
    "dienstleistungen",
    "fehler/störung",
    "information",
    "fall",
    "topdesk",
    "servicefall",
    "wurde",
    "dieser",
}


def compact_text(text: str) -> str:
    return " ".join(str(text or "").split()).strip()


def clean_request_text(text: str) -> str:
    value = compact_text(text)
    if not value:
        return value
    patterns = [
        r"(?i)original service case:\s*[^.]+",
        r"(?i)opened in source system:\s*[^.]+",
        r"(?i)channel:\s*[^.]+",
        r"(?i)category:\s*[^.]+",
        r"(?i)subcategory:\s*[^.]+",
        r"(?i)type:\s*[^.]+",
        r"(?i)impact:\s*[^.]+",
        r"(?i)branch/location:\s*[^.]+",
        r"(?i)imported request context:\s*",
    ]
    for pattern in patterns:
        value = re.sub(pattern, " ", value)
    return compact_text(value)


def normalize_title(title: str) -> str:
    text = compact_text(title).lower()
    text = re.sub(r"\b\d{2,}\b", " ", text)
    text = re.sub(r"[^a-z0-9äöüß/ ]+", " ", text)
    return " ".join(text.split())


def slugify(text: str) -> str:
    text = normalize_title(text).replace("/", " ")
    text = re.sub(r"[^a-z0-9äöüß]+", "-", text).strip("-")
    return text or "unknown"


def top_tokens(texts: list[str], limit: int = 6) -> list[str]:
    counter: Counter[str] = Counter()
    for text in texts:
        for token in TOKEN_RE.findall(text):
            lower = token.lower()
            if lower in STOPWORDS:
                continue
            if re.fullmatch(r"[./-]*\d{1,4}(?:[./-]\d{1,4})+[./-]*", lower):
                continue
            if lower.startswith(".") or lower.endswith(".csv"):
                continue
            counter[lower] += 1
    return [token for token, _count in counter.most_common(limit)]


def load_records(path: Path) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            stripped = line.strip()
            if not stripped:
                continue
            payload = json.loads(stripped)
            if isinstance(payload, dict):
                records.append(payload)
    return records


def build_family_key(record: dict[str, Any]) -> str:
    category = compact_text(record.get("category", "")).lower()
    title = normalize_title(record.get("title", ""))
    if category:
        return f"{category}::{title}"
    return title


def extract_source_refs(record: dict[str, Any]) -> dict[str, Any]:
    return {
        "ticket_key": record.get("ticket_key"),
        "ticket_id": record.get("ticket_id"),
        "title": compact_text(record.get("title", "")),
        "state": compact_text(record.get("state", "")),
        "group": compact_text(record.get("group", "")),
    }


def should_ignore_record(record: dict[str, Any]) -> bool:
    group = compact_text(record.get("group", "")).lower()
    category = compact_text(record.get("category", "")).lower()
    title = compact_text(record.get("title", "")).lower()
    if group == "ctox playground" or category == "ctox playground":
        return True
    if title.startswith("ctox:"):
        return True
    return False


def looks_agentic_note(text: str) -> bool:
    value = compact_text(text).lower()
    if not value:
        return False
    markers = [
        "ich ordne den fall",
        "als nächsten schritt",
        "fuer den naechsten schritt",
        "ohne diese rueckmeldung halte ich",
        "ohne dieses signal halte ich",
        "ich pruefe zuerst",
        "ich kläre",
        "ich klaere",
        "bleibt offen, solange",
    ]
    return any(marker in value for marker in markers)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Extract builder-oriented evidence and candidate gaps from ticket history export."
    )
    parser.add_argument("--input", required=True)
    parser.add_argument("--system", required=True)
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--top-families", type=int, default=12)
    parser.add_argument("--min-family-size", type=int, default=2)
    args = parser.parse_args()

    input_path = Path(args.input)
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    records = load_records(input_path)
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for record in records:
        if should_ignore_record(record):
            continue
        family_key = build_family_key(record)
        if family_key:
            grouped[family_key].append(record)

    ranked = sorted(
        (
            (family_key, items)
            for family_key, items in grouped.items()
            if len(items) >= args.min_family_size
        ),
        key=lambda item: (-len(item[1]), item[0]),
    )[: args.top_families]

    evidence_records: list[dict[str, Any]] = []
    candidate_items: list[dict[str, Any]] = []
    gaps: list[dict[str, Any]] = []
    build_families: list[dict[str, Any]] = []

    for index, (family_key, items) in enumerate(ranked, start=1):
        titles = [compact_text(item.get("title", "")) for item in items]
        requests = [clean_request_text(item.get("request_text", "")) for item in items]
        actions = [
            compact_text(item.get("action_text", ""))
            for item in items
            if compact_text(item.get("action_text", ""))
            and not looks_agentic_note(item.get("action_text", ""))
        ]
        group_counts = Counter(compact_text(item.get("group", "")) for item in items if compact_text(item.get("group", "")))
        state_counts = Counter(compact_text(item.get("state", "")) for item in items if compact_text(item.get("state", "")))
        top_title = Counter(titles).most_common(1)[0][0]
        tokens = top_tokens(titles + requests)
        label = f"HIST-{index:03d}"
        problem_class = slugify(top_title)
        build_families.append(
            {
                "label": label,
                "family_key": family_key,
                "ticket_count": len(items),
                "title": top_title,
                "top_groups": dict(group_counts.most_common(3)),
                "top_states": dict(state_counts.most_common(3)),
                "top_tokens": tokens,
            }
        )
        for local_index, item in enumerate(items, start=1):
            evidence_records.append(
                {
                    "record_id": f"{label}::{local_index}",
                    "source_id": f"ticket-history:{args.system}",
                    "source_type": "ticket_history",
                    "section_ref": label,
                    "page_ref": None,
                    "raw_text": compact_text(item.get("request_text", "")),
                    "normalized_fact": clean_request_text(item.get("request_text", "")) or compact_text(item.get("title", "")),
                    "domain_hint": "runbook_candidate",
                    "confidence": 0.55,
                    "source_ref": extract_source_refs(item),
                }
            )
        candidate_items.append(
            {
                "item_id": f"{args.system}.history.{label.lower()}",
                "label": label,
                "title": top_title,
                "problem_class": problem_class,
                "trigger_phrases": tokens,
                "entry_conditions": [],
                "earliest_blocker": "",
                "expected_guidance": "",
                "tool_actions": [],
                "verification": [],
                "writeback_policy": {},
                "escalate_when": [],
                "supporting_tickets": [extract_source_refs(item) for item in items[:4]],
            }
        )
        gaps.extend(
            [
                {
                    "gap_id": f"{label}::missing_tool_actions",
                    "gap_type": "missing_tool_actions",
                    "summary": f"{label} has repeated ticket evidence but no explicit execution tool path in ticket history.",
                    "affected_source_refs": [extract_source_refs(item) for item in items[:3]],
                    "proposed_resolution": "add runbook-level tool actions from runbooks, manuals, or successful executions",
                    "status": "open",
                },
                {
                    "gap_id": f"{label}::missing_verification",
                    "gap_type": "missing_verification",
                    "summary": f"{label} has no reliable verification rule in ticket history alone.",
                    "affected_source_refs": [extract_source_refs(item) for item in items[:3]],
                    "proposed_resolution": "attach explicit verification steps from execution evidence or manuals",
                    "status": "open",
                },
                {
                    "gap_id": f"{label}::missing_writeback_policy",
                    "gap_type": "missing_writeback_policy",
                    "summary": f"{label} does not expose a stable writeback policy from history alone.",
                    "affected_source_refs": [extract_source_refs(item) for item in items[:3]],
                    "proposed_resolution": "derive channel/writeback rules from skillbook policy, not ticket history",
                    "status": "open",
                },
            ]
        )
        if not actions:
            gaps.append(
                {
                    "gap_id": f"{label}::insufficient_source_evidence",
                    "gap_type": "insufficient_source_evidence",
                    "summary": f"{label} has repeated request evidence but no trustworthy action trace.",
                    "affected_source_refs": [extract_source_refs(item) for item in items[:3]],
                    "proposed_resolution": "pair ticket history with manuals, runbooks, or execution logs before promotion",
                    "status": "open",
                }
            )

    report = {
        "builder_version": "v1-history-extractor",
        "system": args.system,
        "input": str(input_path),
        "record_count": len(records),
        "family_count": len(ranked),
        "families": build_families,
        "candidate_count": len(candidate_items),
        "gap_count": len(gaps),
        "promotion_ready_count": 0,
        "summary": "Ticket history produced candidate families and evidence records, but not promotion-ready runbook items.",
    }

    (output_dir / "history_evidence_records.jsonl").write_text(
        "".join(json.dumps(record, ensure_ascii=False) + "\n" for record in evidence_records),
        encoding="utf-8",
    )
    (output_dir / "history_candidate_runbook_items.jsonl").write_text(
        "".join(json.dumps(item, ensure_ascii=False) + "\n" for item in candidate_items),
        encoding="utf-8",
    )
    (output_dir / "history_build_gaps.json").write_text(
        json.dumps(gaps, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    (output_dir / "history_build_report.json").write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
