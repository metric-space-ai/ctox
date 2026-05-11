#!/usr/bin/env python3
"""Classify technical drawing review feedback and create CTOX learning handoff artifacts."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import subprocess
from pathlib import Path
from typing import Any


SYSTEM = "technical-drawing-review"
SKILL = "technical-drawing-review"
RUNBOOK_ID = "technical-drawing-review-runbook"
SKILLBOOK_ID = "technical-drawing-review-skillbook"


def read_text(args: argparse.Namespace) -> str:
    parts: list[str] = []
    if args.feedback_text:
        parts.append(args.feedback_text)
    for path in args.feedback_file or []:
        parts.append(path.read_text(encoding="utf-8"))
    text = "\n\n".join(part.strip() for part in parts if part and part.strip())
    if not text:
        raise SystemExit("provide --feedback-text or --feedback-file")
    return text


def load_json(path: Path | None) -> Any:
    if not path:
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def slugify(value: str, fallback: str = "feedback") -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return slug[:56] or fallback


def stable_id(*parts: str) -> str:
    digest = hashlib.sha1("\n".join(parts).encode("utf-8")).hexdigest()[:10]
    return digest


def first_sentence(text: str, limit: int = 180) -> str:
    compact = re.sub(r"\s+", " ", text).strip()
    match = re.search(r"(.+?[.!?])(?:\s|$)", compact)
    sentence = (match.group(1) if match else compact)[:limit].strip()
    return sentence or "Human feedback on a technical drawing review."


def contains_any(text: str, patterns: list[str]) -> bool:
    return any(re.search(pattern, text, re.IGNORECASE) for pattern in patterns)


def classify_feedback(text: str) -> dict[str, Any]:
    lowered = text.lower()
    false_positive = contains_any(
        lowered,
        [
            r"\bfalse positive\b",
            r"\bkein(?:e[rsn]?)? fehler\b",
            r"\bnicht falsch\b",
            r"\bist korrekt\b",
            r"\bpasst so\b",
            r"\bbereits abgedeckt\b",
            r"\balready covered\b",
        ],
    )
    missed_issue = contains_any(
        lowered,
        [
            r"\bfehlt\b",
            r"\buebersehen\b",
            r"\bübersehen\b",
            r"\bnicht erkannt\b",
            r"\bmissing\b",
            r"\bmissed\b",
            r"\bshould have flagged\b",
            r"\bzusätzlich\b",
        ],
    )
    future_rule = contains_any(
        lowered,
        [
            r"\bimmer\b",
            r"\bkünftig\b",
            r"\bzukunftig\b",
            r"\bzukünftig\b",
            r"\bab jetzt\b",
            r"\balways\b",
            r"\bfor future\b",
            r"\bstandardmäßig\b",
        ],
    )
    customer_standard = contains_any(
        lowered,
        [
            r"\bbei uns\b",
            r"\bunser(?:e[rsn]?)? standard\b",
            r"\bkundenstandard\b",
            r"\bhouse standard\b",
            r"\bcompany standard\b",
            r"\bkunde\b",
        ],
    )
    review_domain = contains_any(
        lowered,
        [
            r"\btoleranz",
            r"\bpassung",
            r"\boberfl",
            r"\bdatum\b",
            r"\bgd&t\b",
            r"\bform-? und lage\b",
            r"\bzeichnung",
            r"\btitle block\b",
            r"\bschriftfeld\b",
            r"\bmaterial\b",
            r"\bfase\b",
            r"\bentgr",
        ],
    )

    if false_positive and future_rule:
        classification = "false_positive_rule"
    elif missed_issue and future_rule:
        classification = "missed_issue_rule"
    elif future_rule and review_domain:
        classification = "reusable_review_rule"
    elif customer_standard and review_domain:
        classification = "customer_standard_candidate"
    elif false_positive:
        classification = "false_positive_context"
    elif missed_issue:
        classification = "missed_issue_context"
    else:
        classification = "needs_triage"

    reusable = classification in {
        "false_positive_rule",
        "missed_issue_rule",
        "reusable_review_rule",
        "customer_standard_candidate",
    }

    confidence = 0.82 if reusable and review_domain else 0.62 if reusable else 0.48
    if classification == "needs_triage":
        confidence = 0.35

    return {
        "classification": classification,
        "is_reusable_candidate": reusable,
        "confidence": confidence,
        "signals": {
            "false_positive": false_positive,
            "missed_issue": missed_issue,
            "future_rule": future_rule,
            "customer_standard": customer_standard,
            "review_domain": review_domain,
        },
    }


def build_candidate(
    feedback: str,
    classification: dict[str, Any],
    findings: Any,
    manifest: Any,
    review_artifact: Path | None,
    label: str | None,
) -> dict[str, Any]:
    summary = first_sentence(feedback)
    digest = stable_id(summary, classification["classification"])
    item_label = label or f"TDR-FB-{digest[:6].upper()}"
    title = f"Human feedback rule: {summary.rstrip('.')}"
    trigger_phrases = [
        summary,
        classification["classification"].replace("_", " "),
        "technical drawing review feedback",
    ]
    if classification["signals"].get("customer_standard"):
        trigger_phrases.append("customer or house drawing standard")
    if classification["signals"].get("false_positive"):
        trigger_phrases.append("avoid false positive")
    if classification["signals"].get("missed_issue"):
        trigger_phrases.append("missed drawing review issue")

    source_files: list[str] = []
    if review_artifact:
        source_files.append(str(review_artifact))

    evidence = {
        "feedback_excerpt": summary,
        "classification": classification,
        "review_artifact": str(review_artifact) if review_artifact else None,
        "findings_count": len(findings.get("findings", [])) if isinstance(findings, dict) else None,
        "page_count": len(manifest.get("page_images", [])) if isinstance(manifest, dict) else None,
    }

    record = {
        "item_id": f"{RUNBOOK_ID}.{item_label.lower()}",
        "runbook_id": RUNBOOK_ID,
        "skillbook_id": SKILLBOOK_ID,
        "label": item_label,
        "title": title,
        "problem_class": classification["classification"],
        "trigger_phrases": trigger_phrases,
        "entry_conditions": [
            "Human feedback or communication corrects a technical drawing review.",
            "The correction appears repeatable across future technical drawing reviews.",
        ],
        "earliest_blocker": "Feedback is one-off, ambiguous, or lacks enough source evidence to become a repeatable review rule.",
        "expected_guidance": (
            "Before finalizing future reviews, check whether this human correction applies. "
            "If it applies, adapt the finding decision, confidence, or needs_context status and cite visible drawing evidence."
        ),
        "tool_actions": {
            "required": [
                "Query technical-drawing-review runbook knowledge before final annotation.",
                "Compare the current drawing context against this feedback rule.",
                "Keep the rule out of final findings when the visible evidence or customer scope does not match.",
            ],
            "optional": [
                "Attach the human feedback excerpt to the case audit evidence.",
                "Ask for engineering confirmation when the correction conflicts with visible drawing evidence.",
            ],
        },
        "verification": [
            "Rule was derived from human feedback and reviewed before promotion.",
            "Future finding cites visible drawing evidence or marks needs_context.",
            "False positives are not suppressed outside the stated trigger conditions.",
        ],
        "writeback_policy": {
            "artifact": "standalone_html_review",
            "must_include": ["feedback-aware finding decision", "pin evidence", "confidence/status"],
            "knowledge_capture": "Promote only after owner approval or accepted repeated use.",
        },
        "escalate_when": [
            "Feedback conflicts with a visible drawing requirement.",
            "The rule depends on a proprietary customer standard that was not supplied.",
            "The affected feature cannot be pinned on the drawing.",
        ],
        "sources": {
            "feedback": [summary],
            "artifacts": source_files,
        },
        "pages": ["human review feedback"],
        "evidence": evidence,
    }
    record["chunk_text"] = "\n".join(
        [
            f"label: {record['label']}",
            f"title: {record['title']}",
            f"problem_class: {record['problem_class']}",
            f"trigger_phrases: {'; '.join(record['trigger_phrases'])}",
            f"earliest_blocker: {record['earliest_blocker']}",
            f"expected_guidance: {record['expected_guidance']}",
            f"escalate_when: {'; '.join(record['escalate_when'])}",
            "sources/pages: human review feedback",
        ]
    )
    return record


def append_promoted_item(bundle_dir: Path, candidate: dict[str, Any]) -> None:
    runbook_items = bundle_dir / "runbook_items.jsonl"
    runbook_path = bundle_dir / "runbook.json"
    if not runbook_items.exists() or not runbook_path.exists():
        raise SystemExit(f"bundle is missing runbook_items.jsonl or runbook.json: {bundle_dir}")

    existing: list[dict[str, Any]] = []
    with runbook_items.open("r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                existing.append(json.loads(line))
    labels = {item.get("label") for item in existing}
    item_ids = {item.get("item_id") for item in existing}
    if candidate["label"] not in labels and candidate["item_id"] not in item_ids:
        existing.append(candidate)
    with runbook_items.open("w", encoding="utf-8") as handle:
        for item in existing:
            handle.write(json.dumps(item, ensure_ascii=False) + "\n")

    runbook = json.loads(runbook_path.read_text(encoding="utf-8"))
    item_labels = list(runbook.get("item_labels", []))
    if candidate["label"] not in item_labels:
        item_labels.append(candidate["label"])
    runbook["item_labels"] = item_labels
    runbook_path.write_text(json.dumps(runbook, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    report_path = bundle_dir / "build_report.json"
    if report_path.exists():
        report = json.loads(report_path.read_text(encoding="utf-8"))
        report["runbook_knowledge_count"] = len(existing)
        report["items_created"] = len(existing)
        report.setdefault("promoted_feedback_items", [])
        if candidate["label"] not in report["promoted_feedback_items"]:
            report["promoted_feedback_items"].append(candidate["label"])
        report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def run_ctox(args: argparse.Namespace, command: list[str]) -> dict[str, Any]:
    completed = subprocess.run(
        [str(args.ctox_bin), "ticket", *command],
        cwd=args.workspace_root,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        raise SystemExit(completed.stderr.strip() or completed.stdout.strip())
    return json.loads(completed.stdout)


def publish_learning(args: argparse.Namespace, summary: str, candidate: dict[str, Any]) -> dict[str, Any]:
    evidence = {
        "source": "technical-drawing-review feedback ingest",
        "candidate": candidate,
    }
    if args.case_id:
        return run_ctox(
            args,
            [
                "learn-candidate-create",
                "--case-id",
                args.case_id,
                "--summary",
                summary,
                "--actions",
                json.dumps(["update_runbook_item", "adjust_review_prompt"], ensure_ascii=False),
                "--evidence-json",
                json.dumps(evidence, ensure_ascii=False),
            ],
        )
    command = [
        "self-work-put",
        "--system",
        SYSTEM,
        "--kind",
        "runbook-learning-candidate",
        "--title",
        candidate["title"],
        "--body",
        summary,
        "--skill",
        SKILL,
        "--metadata-json",
        json.dumps(evidence, ensure_ascii=False),
    ]
    if args.remote_publish_self_work:
        command.append("--publish")
    return run_ctox(args, command)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--feedback-text")
    parser.add_argument("--feedback-file", type=Path, action="append")
    parser.add_argument("--findings", type=Path)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--review-artifact", type=Path)
    parser.add_argument("--candidate-label")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--promote-to-bundle", type=Path)
    parser.add_argument("--publish", action="store_true")
    parser.add_argument("--remote-publish-self-work", action="store_true")
    parser.add_argument("--case-id")
    parser.add_argument("--ctox-bin", type=Path, default=Path("ctox"))
    parser.add_argument("--workspace-root", type=Path, default=Path.cwd())
    args = parser.parse_args()

    feedback = read_text(args)
    classification = classify_feedback(feedback)
    findings = load_json(args.findings)
    manifest = load_json(args.manifest)
    candidate = build_candidate(
        feedback,
        classification,
        findings,
        manifest,
        args.review_artifact,
        args.candidate_label,
    )
    summary = first_sentence(feedback)
    recommended_action = "promote_runbook_candidate" if classification["is_reusable_candidate"] else "store_as_context_evidence"

    args.output_dir.mkdir(parents=True, exist_ok=True)
    result = {
        "created_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "system": SYSTEM,
        "summary": summary,
        "classification": classification,
        "recommended_action": recommended_action,
        "candidate": candidate if classification["is_reusable_candidate"] else None,
    }

    if args.promote_to_bundle:
        if not classification["is_reusable_candidate"]:
            raise SystemExit("refusing to promote non-reusable feedback")
        append_promoted_item(args.promote_to_bundle, candidate)
        result["promoted_to_bundle"] = str(args.promote_to_bundle)

    if args.publish:
        if not classification["is_reusable_candidate"]:
            raise SystemExit("refusing to publish non-reusable feedback as learning work")
        result["published"] = publish_learning(args, summary, candidate)

    (args.output_dir / "feedback_learning.json").write_text(
        json.dumps(result, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    if classification["is_reusable_candidate"]:
        (args.output_dir / "runbook_candidate.json").write_text(
            json.dumps(candidate, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
