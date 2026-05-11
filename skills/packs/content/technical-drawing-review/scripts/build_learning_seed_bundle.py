#!/usr/bin/env python3
"""Build the CTOX source-skill Skillbook/Runbook seed bundle for drawing review."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


MAIN_SKILL_ID = "technical-drawing-review-main"
SKILLBOOK_ID = "technical-drawing-review-skillbook"
RUNBOOK_ID = "technical-drawing-review-runbook"


def chunk_text(item: dict) -> str:
    fields = [
        ("label", item["label"]),
        ("title", item["title"]),
        ("problem_class", item["problem_class"]),
        ("trigger_phrases", "; ".join(item["trigger_phrases"])),
        ("earliest_blocker", item["earliest_blocker"]),
        ("expected_guidance", item["expected_guidance"]),
        ("escalate_when", "; ".join(item["escalate_when"])),
        ("sources/pages", "; ".join(item["pages"])),
    ]
    return "\n".join(f"{name}: {value}" for name, value in fields)


def item(label: str, title: str, problem_class: str, triggers: list[str], guidance: str, actions: list[str], verification: list[str], escalate: list[str]) -> dict:
    record = {
        "item_id": f"{RUNBOOK_ID}.{label.lower()}",
        "runbook_id": RUNBOOK_ID,
        "skillbook_id": SKILLBOOK_ID,
        "label": label,
        "title": title,
        "problem_class": problem_class,
        "trigger_phrases": triggers,
        "entry_conditions": ["A technical drawing review package is being prepared or reviewed."],
        "earliest_blocker": "Input package cannot be normalized into page images, or the review lacks enough visible evidence to support findings.",
        "expected_guidance": guidance,
        "tool_actions": {
            "required": actions,
            "optional": [
                "Query prior technical-drawing-review runbook items before finalizing findings.",
                "Create a learning candidate when a human correction reveals a reusable review rule.",
            ],
        },
        "verification": verification,
        "writeback_policy": {
            "artifact": "standalone_html_review",
            "must_include": ["embedded page images", "normalized pins", "finding list", "confidence/status"],
            "knowledge_capture": "Promote only verified repeatable review behavior to runbook items; store one-off facts as ticket/context evidence.",
        },
        "escalate_when": escalate,
        "sources": {
            "skill_files": [
                "SKILL.md",
                "references/vision-prompts.md",
                "references/review-checklist.md",
                "references/review-schema.md",
            ]
        },
        "pages": ["technical-drawing-review skill bundle"],
    }
    record["chunk_text"] = chunk_text(record)
    return record


def build_bundle() -> dict:
    items = [
        item(
            "TDR-INPUT",
            "Normalize review packages from PDFs, images, TIFFs, ZIPs, and email attachments",
            "input_normalization",
            ["pdf drawing attachment", "zip of drawings", "multi-page tiff", "email attachments", "multiple drawing files"],
            "Treat the input as one review package. Use prepare_review_inputs.py to extract nested packages and render each drawing sheet to manifest page images before review.",
            ["prepare_review_inputs.py --input <path> --output-dir <work-dir>"],
            ["manifest.json exists", "page_images count matches the review package", "unsupported files are reported"],
            ["no page images were produced", "required attachment format is unsupported", "PDF/TIFF rendering tool is missing"],
        ),
        item(
            "TDR-EXTRACT",
            "Run evidence extraction before judging drawing defects",
            "vision_extraction",
            ["extract drawing evidence", "read title block", "before review", "vision model drawing extraction"],
            "First extract visible evidence for each page: title block, material, notes, dimensions, tolerances, GD&T, manufacturing features, inspection features, and unreadable areas. Do not produce final findings in this pass.",
            ["Run the extraction prompt from references/vision-prompts.md with all page images attached"],
            ["extraction JSON covers every manifest page", "uncertain or unreadable areas are recorded", "no final defects are asserted yet"],
            ["pages are unreadable", "the model skips pages", "extraction invents invisible drawing data"],
        ),
        item(
            "TDR-METADATA",
            "Review title block, revision, sheet, and approval completeness",
            "metadata_review",
            ["missing drawing number", "missing revision", "unchecked drawing", "empty approver", "title block issue"],
            "Check drawing number, part number, title, revision, units, scale, sheet count, author/checker/approver/date, and cross-sheet consistency. Pin the exact title-block field when missing or inconsistent.",
            ["Use extraction JSON plus page image title blocks", "Create metadata findings with normalized pins"],
            ["finding evidence names the visible field", "pin lands on title block field", "cross-sheet issues identify both pages when possible"],
            ["field is unreadable rather than absent", "company release policy is unknown and materiality is unclear"],
        ),
        item(
            "TDR-TOL",
            "Review dimensions, tolerances, fits, and inspectability",
            "dimension_tolerance_review",
            ["missing tolerance", "fit class", "chain dimension", "conflicting dimension", "not inspectable"],
            "Check for missing governing tolerance, ambiguous fit, conflicting or duplicate dimensions, risky chain dimensions, and features that cannot be inspected from a clear setup. Do not flag missing local tolerances when a visible general tolerance clearly governs them.",
            ["Apply review-checklist dimensioning and tolerance sections", "Use needs_context for design-intent-dependent concerns"],
            ["finding ties to a visible dimension or tolerance note", "governing general tolerance was checked", "confidence is lower for uncertain OCR"],
            ["general tolerance note is unreadable", "mating part or functional requirement is unavailable"],
        ),
        item(
            "TDR-FINISH",
            "Review material, finish, heat treatment, surface, and edge requirements",
            "material_finish_review",
            ["missing material", "surface finish", "heat treatment", "edge break", "deburr", "coating"],
            "Focus on material grade, heat treatment, coating, hardness, surface finish, deburring, and edge requirements. Do not flag every unspecific surface; prioritize functional surfaces or missing governing notes.",
            ["Apply review-checklist material and surface sections", "Pin functional surface or governing note area"],
            ["finding explains why the surface/material requirement matters", "generic best-practice comments are filtered", "needs_context is used for process-dependent concerns"],
            ["function of the surface is unknown", "customer standard may define default finish but is unavailable"],
        ),
        item(
            "TDR-MFG",
            "Review process-specific manufacturability only when process context supports it",
            "manufacturability_review",
            ["machining risk", "sheet metal", "weldment", "casting", "additive", "tool access", "sharp internal corner"],
            "Use the intended process if provided. If process is unknown, phrase process-specific concerns as needs_context. Distinguish visible geometry risk from assumed manufacturing strategy.",
            ["Classify likely drawing/process type", "Apply the matching manufacturing checklist section"],
            ["finding cites visible geometry", "process assumptions are explicit", "needs_context is used when process is unknown"],
            ["no manufacturing process is known", "geometry is too cropped or low-resolution to inspect"],
        ),
        item(
            "TDR-ANNOTATE",
            "Finalize high-signal pinned findings and standalone HTML",
            "annotation_packaging",
            ["standalone html", "pins on drawing", "finding json", "review artifact"],
            "Deduplicate candidates, keep high-signal findings, calibrate confidence, validate findings JSON, then generate one offline HTML with embedded page images and interactive pins.",
            ["validate_findings.py findings.json", "generate_review_html.py --findings findings.json --manifest manifest.json --output review.html"],
            ["JSON validates", "HTML has embedded data URLs", "pins and issue cards navigate both ways", "no external links/scripts are required"],
            ["finding count is noisy", "pins are approximate", "HTML contains external dependencies"],
        ),
        item(
            "TDR-LEARN",
            "Promote verified recurring review corrections into runbook knowledge",
            "learning_loop",
            ["human corrected finding", "false positive", "missed drawing issue", "update review rules", "learn from review"],
            "After a human review, run ingest_review_feedback.py to classify learning. One-off drawing facts stay as ticket/context evidence. Repeatable review procedure becomes a runbook candidate or self-work item only when it has stable scope, trigger, tool actions, verification, and source evidence.",
            ["ingest_review_feedback.py --feedback-file <feedback> --findings <findings.json> --manifest <manifest.json> --review-artifact <review.html> --output-dir <work-dir>/feedback", "For case-bound feedback use --publish --case-id <case-id>", "For approved reusable learning use --promote-to-bundle <bundle-dir> and re-import with ctox ticket source-skill-import-bundle"],
            ["feedback_learning.json exists", "candidate has source evidence", "candidate label is stable", "verification and writeback policy are explicit", "import updates knowledge_runbook_items"],
            ["feedback is anecdotal", "no source artifact exists", "correction depends on a one-off customer preference"],
        ),
    ]

    return {
        "main_skill": {
            "main_skill_id": MAIN_SKILL_ID,
            "title": "Technical Drawing Review",
            "primary_channel": "artifact",
            "entry_action": "resolve_runbook_item",
            "resolver_contract": {
                "mode": "runbook-item",
                "retrieval_unit": "runbook_item",
                "query_sources": ["review request", "attachment manifest", "drawing type", "human feedback"],
            },
            "execution_contract": {
                "output": "standalone interactive HTML review plus validated findings JSON",
                "must_use": ["vision prompt pipeline", "manifest page images", "schema validation"],
            },
            "resolve_flow": [
                "Normalize inputs to a manifest.",
                "Query prior runbook items for matching drawing/review issue family.",
                "Run extraction, review, and annotation prompts.",
                "Validate JSON and package standalone HTML.",
                "Capture verified repeatable corrections as runbook learning candidates.",
            ],
            "writeback_flow": [
                "Return the standalone HTML path and summarize validation.",
                "Do not claim final engineering approval.",
                "Persist reusable learning through Skillbook/Runbook records, not workspace notes.",
            ],
            "linked_skillbooks": [SKILLBOOK_ID],
            "linked_runbooks": [RUNBOOK_ID],
        },
        "skillbook": {
            "skillbook_id": SKILLBOOK_ID,
            "title": "Technical Drawing Review Skillbook",
            "version": "v1",
            "mission": "Guide CTOX technical drawing reviews from mixed attachment packages to cautious, evidence-grounded, pinned findings and standalone HTML artifacts while preserving reusable review learning as Runbook knowledge.",
            "non_negotiable_rules": [
                "Normalize every review package before model review.",
                "Use visible evidence only; do not invent drawing data.",
                "Use needs_context when design intent, process capability, or standards are missing.",
                "Pin every final finding to visible evidence.",
                "Promote only verified repeatable procedures to Runbook knowledge.",
            ],
            "runtime_policy": "Resolve the most relevant runbook item before finalizing a drawing review. Load prior learning when the package, drawing type, or issue family matches.",
            "answer_contract": "Deliver a validated findings JSON and one standalone HTML review artifact with embedded pages and pins. Include uncertainty and blockers plainly.",
            "workflow_backbone": [
                "Normalize package",
                "Retrieve matching runbook item",
                "Extract visible evidence",
                "Review against checklist",
                "Finalize pinned JSON",
                "Generate standalone HTML",
                "Capture verified learning",
            ],
            "routing_taxonomy": [
                "input_normalization",
                "vision_extraction",
                "metadata_review",
                "dimension_tolerance_review",
                "material_finish_review",
                "manufacturability_review",
                "annotation_packaging",
                "learning_loop",
            ],
            "linked_runbooks": [RUNBOOK_ID],
        },
        "runbook": {
            "runbook_id": RUNBOOK_ID,
            "skillbook_id": SKILLBOOK_ID,
            "title": "Technical Drawing Review Runbook",
            "version": "v1",
            "status": "active",
            "problem_domain": "Mechanical technical drawing review and handoff package QA",
            "item_labels": [record["label"] for record in items],
        },
        "items": items,
    }


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    bundle = build_bundle()
    write_json(args.output_dir / "main_skill.json", bundle["main_skill"])
    write_json(args.output_dir / "skillbook.json", bundle["skillbook"])
    write_json(args.output_dir / "runbook.json", bundle["runbook"])
    with (args.output_dir / "runbook_items.jsonl").open("w", encoding="utf-8") as handle:
        for record in bundle["items"]:
            handle.write(json.dumps(record, ensure_ascii=False) + "\n")
    write_json(
        args.output_dir / "build_report.json",
        {
            "builder_version": "technical-drawing-review-seed-v1",
            "sources": ["technical-drawing-review skill bundle"],
            "evidence_record_count": 4,
            "skillbook_knowledge_count": len(bundle["skillbook"]["non_negotiable_rules"]),
            "runbook_knowledge_count": len(bundle["items"]),
            "items_created": len(bundle["items"]),
            "items_rejected": 0,
            "gaps_open": [],
            "embedding_ready_items": len(bundle["items"]),
            "sqlite_upserts": {
                "knowledge_main_skills": 1,
                "knowledge_skillbooks": 1,
                "knowledge_runbooks": 1,
                "knowledge_runbook_items": len(bundle["items"]),
            },
        },
    )
    print(args.output_dir)


if __name__ == "__main__":
    main()
