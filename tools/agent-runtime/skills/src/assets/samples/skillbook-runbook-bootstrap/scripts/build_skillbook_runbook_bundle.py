#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


BUILDER_VERSION = "v1"
RUNBOOK_LABEL_RE = re.compile(r"^###\s+([A-Z]+-\d+)\s+(.+)$")


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def lines(text: str) -> list[str]:
    return [line.rstrip() for line in text.splitlines()]


def compact_text(text: str) -> str:
    return " ".join(str(text or "").split()).strip()


def parse_key_value_block(text: str) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in lines(text):
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        result[key.strip().lower()] = value.strip()
    return result


def parse_bullets(block: str) -> list[str]:
    items: list[str] = []
    for line in lines(block):
        stripped = line.strip()
        if stripped.startswith("- "):
            items.append(stripped[2:].strip())
        elif stripped.startswith(tuple(f"{n}. " for n in range(1, 10))):
            items.append(stripped.split(". ", 1)[1].strip())
    return [item for item in items if item]


def section_slice(text: str, header: str, next_headers: list[str]) -> str:
    start = text.find(header)
    if start == -1:
        return ""
    start += len(header)
    end = len(text)
    for candidate in next_headers:
        idx = text.find(candidate, start)
        if idx != -1 and idx < end:
            end = idx
    return text[start:end].strip()


def split_subsections(block_lines: list[str]) -> dict[str, list[str]]:
    sections: dict[str, list[str]] = {}
    current: str | None = None
    for raw in block_lines:
        line = raw.strip()
        if not line:
            continue
        if line.endswith(":"):
            current = line[:-1].strip().lower()
            sections[current] = []
            continue
        if current is not None:
            sections[current].append(raw)
    return sections


def deterministic_chunk_text(item: dict[str, Any]) -> str:
    sources = item.get("sources") or []
    source_titles = []
    for source in sources:
        if isinstance(source, dict):
            title = compact_text(source.get("title", ""))
            if title:
                source_titles.append(title)
    pages = [compact_text(page) for page in item.get("pages") or [] if compact_text(page)]
    return "\n".join(
        [
            compact_text(item["label"]),
            compact_text(item["title"]),
            compact_text(item["problem_class"]),
            " | ".join(compact_text(entry) for entry in item.get("trigger_phrases") or [] if compact_text(entry)),
            compact_text(item.get("earliest_blocker", "")),
            compact_text(item.get("expected_guidance", "")),
            " | ".join(compact_text(entry) for entry in item.get("escalate_when") or [] if compact_text(entry)),
            " | ".join(source_titles + pages),
        ]
    ).strip()


@dataclass
class EvidenceRecord:
    record_id: str
    source_id: str
    source_type: str
    section_ref: str
    page_ref: str | None
    raw_text: str
    normalized_fact: str
    domain_hint: str
    confidence: float

    def as_dict(self) -> dict[str, Any]:
        return {
            "record_id": self.record_id,
            "source_id": self.source_id,
            "source_type": self.source_type,
            "section_ref": self.section_ref,
            "page_ref": self.page_ref,
            "raw_text": self.raw_text,
            "normalized_fact": self.normalized_fact,
            "domain_hint": self.domain_hint,
            "confidence": self.confidence,
        }


def build_source_descriptor(path: Path, source_type: str) -> dict[str, Any]:
    return {
        "source_id": f"{source_type}:{path.stem}",
        "source_type": source_type,
        "title": path.name,
        "uri": str(path),
        "version": None,
    }


def parse_skillbook(path: Path, skillbook_id: str) -> tuple[dict[str, Any], list[EvidenceRecord], dict[str, Any]]:
    text = read_text(path)
    meta = parse_key_value_block("\n".join(lines(text)[:8]))
    source = build_source_descriptor(path, "markdown_skillbook")
    mission = section_slice(
        text,
        "## 1. Mission",
        ["## 2. Non-Negotiable Rules", "## 2. Non Negotiable Rules"],
    )
    rules_block = section_slice(text, "## 2. Non-Negotiable Rules", ["## 3. Runtime Policy"])
    runtime_policy = section_slice(text, "## 3. Runtime Policy", ["## 4. Answer Contract"])
    answer_contract = section_slice(text, "## 4. Answer Contract", ["## 5. Manual Scope"])
    routing_taxonomy = parse_bullets(
        section_slice(text, "## 6. Intent Router", ["## 7. Workflow Backbone"])
    )
    workflow_backbone = parse_bullets(
        section_slice(text, "## 7. Workflow Backbone", ["## 8. Scenario Library"])
    )
    skillbook = {
        "skillbook_id": skillbook_id,
        "title": meta.get("primary use", path.stem),
        "version": meta.get("version", "1.0"),
        "mission": compact_text(mission),
        "non_negotiable_rules": parse_bullets(rules_block),
        "runtime_policy": compact_text(runtime_policy),
        "answer_contract": compact_text(answer_contract),
        "workflow_backbone": workflow_backbone,
        "routing_taxonomy": routing_taxonomy,
    }
    evidence = [
        EvidenceRecord(
            record_id=f"{source['source_id']}::mission",
            source_id=source["source_id"],
            source_type=source["source_type"],
            section_ref="mission",
            page_ref=None,
            raw_text=mission,
            normalized_fact=compact_text(mission),
            domain_hint="skillbook.mission",
            confidence=1.0,
        ),
        EvidenceRecord(
            record_id=f"{source['source_id']}::rules",
            source_id=source["source_id"],
            source_type=source["source_type"],
            section_ref="non_negotiable_rules",
            page_ref=None,
            raw_text=rules_block,
            normalized_fact=" | ".join(skillbook["non_negotiable_rules"]),
            domain_hint="skillbook.rules",
            confidence=1.0,
        ),
        EvidenceRecord(
            record_id=f"{source['source_id']}::runtime_policy",
            source_id=source["source_id"],
            source_type=source["source_type"],
            section_ref="runtime_policy",
            page_ref=None,
            raw_text=runtime_policy,
            normalized_fact=skillbook["runtime_policy"],
            domain_hint="skillbook.runtime_policy",
            confidence=1.0,
        ),
        EvidenceRecord(
            record_id=f"{source['source_id']}::answer_contract",
            source_id=source["source_id"],
            source_type=source["source_type"],
            section_ref="answer_contract",
            page_ref=None,
            raw_text=answer_contract,
            normalized_fact=skillbook["answer_contract"],
            domain_hint="skillbook.answer_contract",
            confidence=1.0,
        ),
    ]
    knowledge = {
        "skillbook_knowledge": {
            "mission": skillbook["mission"],
            "non_negotiable_rules": skillbook["non_negotiable_rules"],
            "runtime_policy": skillbook["runtime_policy"],
            "answer_contract": skillbook["answer_contract"],
            "workflow_backbone": workflow_backbone,
            "routing_taxonomy": routing_taxonomy,
        },
        "source": source,
    }
    return skillbook, evidence, knowledge


def parse_runbook(path: Path, runbook_id: str, skillbook_id: str) -> tuple[dict[str, Any], list[dict[str, Any]], list[EvidenceRecord], dict[str, Any]]:
    text = read_text(path)
    meta = parse_key_value_block("\n".join(lines(text)[:10]))
    source = build_source_descriptor(path, "markdown_runbook")
    scenarios: dict[str, dict[str, Any]] = {}
    current: dict[str, Any] | None = None
    for line in lines(text):
        match = RUNBOOK_LABEL_RE.match(line.strip())
        if match:
            current = {"label": match.group(1), "title": match.group(2).strip(), "body": []}
            scenarios[current["label"]] = current
            continue
        if current is not None:
            current["body"].append(line)

    items: list[dict[str, Any]] = []
    evidence: list[EvidenceRecord] = []
    candidate_items: list[dict[str, Any]] = []
    for label, raw in scenarios.items():
        sections = split_subsections(raw["body"])
        trigger_phrases = parse_bullets("\n".join(sections.get("typische nutzerformulierungen", [])))
        pages = [
            entry.strip("- ").strip()
            for entry in sections.get("manual-seiten", [])
            if entry.strip()
        ]
        escalate_when = parse_bullets("\n".join(sections.get("eskalieren wenn", [])))
        earliest_blocker = compact_text("\n".join(sections.get("fruehester blocker", [])))
        expected_guidance = compact_text("\n".join(sections.get("bot-antwortbaustein", [])))
        problem_prefix = label.split("-", 1)[0].lower()
        item = {
            "item_id": f"{runbook_id}.{label.lower()}",
            "runbook_id": runbook_id,
            "skillbook_id": skillbook_id,
            "label": label,
            "title": raw["title"],
            "problem_class": f"{problem_prefix}.{label.lower()}",
            "trigger_phrases": trigger_phrases,
            "entry_conditions": [],
            "earliest_blocker": earliest_blocker,
            "expected_guidance": expected_guidance,
            "tool_actions": [
                {
                    "tool": "channel.send",
                    "mode": "suggest_or_draft",
                    "target": "email",
                }
            ],
            "verification": [
                "Reply follows the answer contract from the linked skillbook.",
                "UI labels remain exact where applicable.",
            ],
            "writeback_policy": {
                "channel": "email",
                "default_mode": "draft",
            },
            "escalate_when": escalate_when,
            "sources": [
                {
                    "title": path.name,
                    "path": str(path),
                }
            ],
            "pages": pages,
        }
        item["chunk_text"] = deterministic_chunk_text(item)
        items.append(item)
        candidate_items.append(item.copy())
        evidence.append(
            EvidenceRecord(
                record_id=f"{source['source_id']}::{label}",
                source_id=source["source_id"],
                source_type=source["source_type"],
                section_ref=label,
                page_ref=", ".join(pages) if pages else None,
                raw_text="\n".join(raw["body"]).strip(),
                normalized_fact=compact_text(expected_guidance or earliest_blocker or raw["title"]),
                domain_hint=f"runbook.{label.lower()}",
                confidence=1.0,
            )
        )

    runbook = {
        "runbook_id": runbook_id,
        "skillbook_id": skillbook_id,
        "title": meta.get("zielsystem", path.stem),
        "version": meta.get("version", "1.0"),
        "status": meta.get("status", "draft"),
        "problem_domain": meta.get("zielsystem", "unknown"),
        "item_labels": [item["label"] for item in items],
    }
    knowledge = {
        "runbook_knowledge": [
            {
                "label": item["label"],
                "problem_class": item["problem_class"],
                "trigger_phrases": item["trigger_phrases"],
                "earliest_blocker": item["earliest_blocker"],
                "expected_guidance": item["expected_guidance"],
                "tool_actions": item["tool_actions"],
                "verification": item["verification"],
                "writeback_policy": item["writeback_policy"],
                "escalate_when": item["escalate_when"],
            }
            for item in items
        ],
        "source": source,
    }
    return runbook, candidate_items, evidence, knowledge


def validate_candidate_items(candidate_items: list[dict[str, Any]]) -> tuple[list[dict[str, Any]], list[dict[str, Any]], list[dict[str, Any]]]:
    live_labels: set[str] = set()
    valid_items: list[dict[str, Any]] = []
    rejected_items: list[dict[str, Any]] = []
    gaps: list[dict[str, Any]] = []
    for item in candidate_items:
        reasons: list[str] = []
        label = compact_text(item.get("label", ""))
        if not label:
            reasons.append("missing_label")
        elif label in live_labels:
            reasons.append("ambiguous_boundary")
        title = compact_text(item.get("title", ""))
        if not title:
            reasons.append("insufficient_source_evidence")
        if not compact_text(item.get("earliest_blocker", "")):
            reasons.append("insufficient_source_evidence")
        if not compact_text(item.get("expected_guidance", "")):
            reasons.append("insufficient_source_evidence")
        if not item.get("tool_actions"):
            reasons.append("missing_tool_actions")
        if not item.get("verification"):
            reasons.append("missing_verification")
        if not item.get("writeback_policy"):
            reasons.append("missing_writeback_policy")
        chunk_text = deterministic_chunk_text(item)
        if not chunk_text:
            reasons.append("ambiguous_boundary")
        item["chunk_text"] = chunk_text
        if reasons:
            unique = sorted(set(reasons))
            rejected_items.append(
                {
                    "label": label or None,
                    "title": title or None,
                    "problem_class": item.get("problem_class"),
                    "reasons": unique,
                }
            )
            for index, reason in enumerate(unique, start=1):
                gaps.append(
                    {
                        "gap_id": f"{item.get('item_id', 'item')}::{index}::{reason}",
                        "gap_type": reason,
                        "summary": f"{label or 'unlabeled item'} failed validation: {reason}",
                        "affected_source_refs": [label] if label else [],
                        "proposed_resolution": "split, complete, or reject the candidate before promotion",
                        "status": "open",
                    }
                )
            continue
        live_labels.add(label)
        valid_items.append(item)
    return valid_items, rejected_items, gaps


def build_main_skill(main_skill_id: str, skillbook_id: str, runbook_id: str) -> dict[str, Any]:
    return {
        "main_skill_id": main_skill_id,
        "title": "Support main skill",
        "primary_channel": "email",
        "entry_action": "resolve_runbook_item",
        "resolver_contract": {
            "mode": "runbook-item",
            "retrieval_unit": "runbook_item",
            "decision_rule": "Load exactly one best item when confidence is clear, otherwise mark needs_review.",
        },
        "execution_contract": {
            "target": "channel_reply",
            "output_mode_order": ["suggestion", "draft", "send"],
            "required_output_fields": [
                "decision",
                "matched_label",
                "reply_subject",
                "reply_body",
            ],
        },
        "resolve_flow": [
            "Summarize the inbound problem as a runbook-item query.",
            "Retrieve the best matching runbook item.",
            "Load parent runbook and skillbook.",
            "Compose the output or mark needs_review.",
        ],
        "writeback_flow": [
            "Verify the prepared output.",
            "Write back only through the configured channel policy.",
        ],
        "linked_skillbooks": [skillbook_id],
        "linked_runbooks": [runbook_id],
    }


def write_json(path: Path, payload: Any) -> None:
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def write_jsonl(path: Path, payloads: list[dict[str, Any]]) -> None:
    with path.open("w", encoding="utf-8") as handle:
        for payload in payloads:
            handle.write(json.dumps(payload, ensure_ascii=False) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--skillbook", required=True)
    parser.add_argument("--runbook", required=True)
    parser.add_argument("--main-skill-id", required=True)
    parser.add_argument("--skillbook-id", required=True)
    parser.add_argument("--runbook-id", required=True)
    parser.add_argument("--output-dir", required=True)
    args = parser.parse_args()

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    skillbook, skillbook_evidence, skillbook_knowledge = parse_skillbook(
        Path(args.skillbook), args.skillbook_id
    )
    runbook, candidate_items, runbook_evidence, runbook_knowledge = parse_runbook(
        Path(args.runbook), args.runbook_id, args.skillbook_id
    )
    valid_items, rejected_items, gaps = validate_candidate_items(candidate_items)
    skillbook["linked_runbooks"] = [args.runbook_id]
    runbook["item_labels"] = [item["label"] for item in valid_items]
    main_skill = build_main_skill(args.main_skill_id, args.skillbook_id, args.runbook_id)

    evidence_records = [record.as_dict() for record in skillbook_evidence + runbook_evidence]
    knowledge_separation = {
        "skillbook_knowledge": skillbook_knowledge["skillbook_knowledge"],
        "runbook_knowledge": runbook_knowledge["runbook_knowledge"],
    }

    write_json(output_dir / "main_skill.json", main_skill)
    write_json(output_dir / "skillbook.json", skillbook)
    write_json(output_dir / "runbook.json", runbook)
    write_jsonl(output_dir / "runbook_items.jsonl", valid_items)
    write_jsonl(output_dir / "evidence_records.jsonl", evidence_records)
    write_json(output_dir / "knowledge_separation.json", knowledge_separation)
    write_jsonl(output_dir / "candidate_runbook_items.jsonl", candidate_items)

    build_report = {
        "builder_version": BUILDER_VERSION,
        "sources": [
            skillbook_knowledge["source"],
            runbook_knowledge["source"],
        ],
        "evidence_record_count": len(evidence_records),
        "skillbook_knowledge_count": sum(
            1
            for value in knowledge_separation["skillbook_knowledge"].values()
            if value
        ),
        "runbook_knowledge_count": len(knowledge_separation["runbook_knowledge"]),
        "artifacts_written": [
            "main_skill.json",
            "skillbook.json",
            "runbook.json",
            "runbook_items.jsonl",
            "evidence_records.jsonl",
            "knowledge_separation.json",
            "candidate_runbook_items.jsonl",
            "build_report.json",
        ],
        "items_created": [
            {
                "item_id": item["item_id"],
                "label": item["label"],
                "problem_class": item["problem_class"],
            }
            for item in valid_items
        ],
        "items_rejected": rejected_items,
        "gaps_open": gaps,
        "embedding_ready_items": [item["item_id"] for item in valid_items],
        "sqlite_upserts": {
            "knowledge_main_skills": 1,
            "knowledge_skillbooks": 1,
            "knowledge_runbooks": 1,
            "knowledge_runbook_items": len(valid_items),
            "knowledge_sources": 2,
            "knowledge_item_sources": len(valid_items),
        },
    }
    write_json(output_dir / "build_report.json", build_report)

    print(
        json.dumps(
            {
                "main_skill": str(output_dir / "main_skill.json"),
                "skillbook": str(output_dir / "skillbook.json"),
                "runbook": str(output_dir / "runbook.json"),
                "runbook_items": str(output_dir / "runbook_items.jsonl"),
                "build_report": str(output_dir / "build_report.json"),
                "item_count": len(valid_items),
                "gap_count": len(gaps),
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
