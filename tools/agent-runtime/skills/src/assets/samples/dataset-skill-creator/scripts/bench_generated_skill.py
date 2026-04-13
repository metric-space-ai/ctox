#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[4]
FIELD_LEAK_PATTERNS = {
    "internal_field_names": re.compile(
        r"`(?:triage_focus|handling_steps|decision_support|operator_summary|family_key|"
        r"historical_examples|close_when|note_guidance|caution_signals)`"
    ),
    "code_style_identifiers": re.compile(r"`[a-z0-9]+(?:_[a-z0-9]+){1,}`"),
    "tooling_terms": re.compile(r"\b(?:sqlite|json dump|parser|tooling internals|yaml|dataclass)\b", re.IGNORECASE),
    "meta_jargon": re.compile(r"\b(?:caution signals|operational nouns)\b", re.IGNORECASE),
    "analysis_voice": re.compile(r"\b(?:operators treat this as|ticket family is|evidence shows|historically observed)\b", re.IGNORECASE),
    "raw_history_fragments": re.compile(r"\b\d{2}\.\d{2}\.\d{4}\b|:\s*(?:Hallo|Hi|Guten Tag)\b"),
}
GENERIC_FAMILY_PARTS = {"", "-", "--", "unknown", "general", "uncategorized", "ticket", "users", "user", "n/a", "na"}


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load module from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def extract_sections(skill_text: str) -> dict[str, bool]:
    required = [
        "## How To Handle A New Ticket",
        "## What To Do First",
        "## Priority Families",
        "## How To Write Internal Notes",
        "## When To Escalate",
        "## Common Failure Modes",
    ]
    return {section: section in skill_text for section in required}


def build_case_query(title: str, request: str) -> str:
    request = re.sub(r"\s+", " ", str(request or "").strip())
    title = re.sub(r"\s+", " ", str(title or "").strip())
    if request:
        return f"{title}. {request[:220]}"
    return title


def load_family_rows(input_path: Path, min_family_size: int) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    module = load_module(
        "ticket_operating_model_build",
        REPO_ROOT / "skills/system/ticket-operating-model-bootstrap/scripts/build_ticket_operating_model.py",
    )
    _sheet_name, rows = module.load_rows(input_path)
    headers = rows[0].values.keys()
    title_col = module.find_column(headers, "kurzbeschreibung", "details", "title", "subject")
    request_col = module.find_column(headers, "anfrage", "request_text", "body_text", "body", "request")
    request_type_col = module.find_column(
        headers,
        "art 'servicefall'",
        "art servicefall",
        "request_type",
        "ticket_type",
        "type",
    )
    category_col = module.find_column(headers, "kategorie", "category", "group", "group_name", "queue")
    subcategory_col = module.find_column(
        headers,
        "unterkategorie",
        "subcategory",
        "sub_type",
        "label",
        "primary_label",
    )
    grouped: dict[str, list[Any]] = defaultdict(list)
    for row in rows:
        grouped[module.family_key(row, request_type_col, category_col, subcategory_col, title_col, request_col)].append(row)
    families = []
    for family_key, family_rows in grouped.items():
        if len(family_rows) < min_family_size:
            continue
        sample = family_rows[0]
        families.append(
            {
                "family_key": family_key,
                "row_count": len(family_rows),
                "canonical_ticket_id": sample.ticket_id,
                "title": module.ticket_title(sample, title_col),
                "query": build_case_query(module.ticket_title(sample, title_col), sample.get(request_col) or ""),
            }
        )
    families.sort(key=lambda item: item["row_count"], reverse=True)
    return families, rows


def query_family(model_dir: Path, query: str) -> dict[str, Any]:
    command = [
        sys.executable,
        str(REPO_ROOT / "skills/system/ticket-operating-model-bootstrap/scripts/query_ticket_operating_model.py"),
        "--model-dir",
        str(model_dir),
        "--query",
        query,
        "--top-k",
        "1",
    ]
    completed = subprocess.run(command, cwd=REPO_ROOT, capture_output=True, text=True, check=False)
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or completed.stdout.strip() or "query command failed")
    return json.loads(completed.stdout)


def decision_support_complete(family_result: dict[str, Any]) -> bool:
    decision = family_result.get("decision_support") or {}
    return bool(
        decision.get("operator_summary")
        and decision.get("triage_focus")
        and decision.get("handling_steps")
        and decision.get("close_when")
        and decision.get("note_guidance")
    )


def content_leak_free(skill_text: str) -> bool:
    lowered = skill_text.lower()
    forbidden = ["sqlite", "parser", "json dump", "tooling internals", "reference commands"]
    return not any(token in lowered for token in forbidden)


def generic_family_key(family_key: str | None) -> bool:
    if not family_key:
        return True
    parts = [part.strip().lower() for part in str(family_key).split("::")]
    meaningful = [part for part in parts if part and part not in GENERIC_FAMILY_PARTS]
    return len(meaningful) < 2


def language_review(skill_text: str) -> dict[str, Any]:
    findings: list[dict[str, Any]] = []
    for label, pattern in FIELD_LEAK_PATTERNS.items():
        for match in pattern.finditer(skill_text):
            findings.append(
                {
                    "kind": label,
                    "excerpt": shorten_excerpt(match.group(0)),
                }
            )
    return {
        "findings": findings,
        "clean": not findings,
    }


def shorten_excerpt(text: str, limit: int = 120) -> str:
    text = re.sub(r"\s+", " ", text.strip())
    return text if len(text) <= limit else text[: limit - 3] + "..."


def render_markdown(summary: dict[str, Any], cases: list[dict[str, Any]]) -> str:
    lines = ["# Generated Skill Benchmark", ""]
    lines.append(f"- Skill: `{summary['skill_dir']}`")
    lines.append(f"- Cases: `{summary['case_count']}`")
    lines.append(f"- Top-1 family hit rate: `{summary['top1_hit_rate']:.2f}`")
    lines.append(f"- Decision support completeness: `{summary['decision_support_completeness']:.2f}`")
    lines.append(f"- Required sections present: `{summary['all_required_sections_present']}`")
    lines.append(f"- Content leak free: `{summary['content_leak_free']}`")
    lines.append(f"- Language review clean: `{summary['language_review_clean']}`")
    lines.append(f"- Generic family cases: `{summary['generic_family_case_count']}`")
    lines.append("")
    if summary["language_findings"]:
        lines.append("## Language Findings")
        lines.append("")
        for finding in summary["language_findings"]:
            lines.append(f"- `{finding['kind']}`: {finding['excerpt']}")
        lines.append("")
    lines.append("## Cases")
    lines.append("")
    for case in cases:
        lines.append(f"### {case['expected_family']}")
        lines.append("")
        lines.append(f"- Query: {case['query']}")
        lines.append(f"- Top family: {case.get('top_family')}")
        lines.append(f"- Match: {case['top1_match']}")
        lines.append(f"- Decision support complete: {case['decision_support_complete']}")
        lines.append("")
    return "\n".join(lines) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Benchmark a generated dataset skill for operational usefulness.")
    parser.add_argument("--skill-dir", required=True)
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--input")
    parser.add_argument("--input-xlsx")
    parser.add_argument("--cases", type=int, default=8)
    parser.add_argument("--min-family-size", type=int, default=25)
    parser.add_argument("--output")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    skill_dir = Path(args.skill_dir)
    model_dir = Path(args.model_dir)
    input_raw = args.input or args.input_xlsx
    if not input_raw:
        raise SystemExit("one of --input or --input-xlsx is required")
    input_path = Path(input_raw)
    skill_text = (skill_dir / "SKILL.md").read_text(encoding="utf-8")
    family_highlights_path = skill_dir / "references" / "family-highlights.md"
    family_highlights_text = family_highlights_path.read_text(encoding="utf-8") if family_highlights_path.exists() else ""
    review_text = "\n".join(part for part in [skill_text, family_highlights_text] if part)
    section_state = extract_sections(skill_text)
    review = language_review(review_text)
    families, _rows = load_family_rows(input_path, args.min_family_size)
    selected = families[: args.cases]
    case_results = []
    complete_count = 0
    hit_count = 0
    for family in selected:
        payload = query_family(model_dir, family["query"])
        families_out = payload.get("families") or []
        top = families_out[0] if families_out else {}
        top_family = top.get("family_key")
        top_match = top_family == family["family_key"]
        support_complete = decision_support_complete(top)
        if top_match:
            hit_count += 1
        if support_complete:
            complete_count += 1
        case_results.append(
            {
                "expected_family": family["family_key"],
                "query": family["query"],
                "top_family": top_family,
                "top1_match": top_match,
                "decision_support_complete": support_complete,
                "generic_family": generic_family_key(family["family_key"]) or generic_family_key(top_family),
            }
        )
    summary = {
        "skill_dir": str(skill_dir),
        "case_count": len(case_results),
        "top1_hit_rate": hit_count / max(1, len(case_results)),
        "decision_support_completeness": complete_count / max(1, len(case_results)),
        "all_required_sections_present": all(section_state.values()),
        "missing_sections": [name for name, present in section_state.items() if not present],
        "content_leak_free": content_leak_free(review_text),
        "language_review_clean": review["clean"],
        "language_findings": review["findings"],
        "generic_family_case_count": sum(1 for case in case_results if case["generic_family"]),
    }
    markdown = render_markdown(summary, case_results)
    if args.output:
        output = Path(args.output)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(markdown, encoding="utf-8")
        output.with_suffix(".json").write_text(
            json.dumps({"summary": summary, "cases": case_results}, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
    print(json.dumps({"summary": summary, "cases": case_results}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
