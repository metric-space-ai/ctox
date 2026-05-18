#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import shutil
from pathlib import Path
from typing import Any


VALID_ARCHETYPES = {"operating-model", "lookup-reference", "workflow", "policy-gate"}
LEAKY_TERMS = re.compile(
    r"\b(?:sqlite|json dump|parser|yaml|tooling internals|reference commands|ctox ticket)\b",
    re.IGNORECASE,
)
FIELD_NAME_PATTERN = re.compile(
    r"`(?:triage_focus|handling_steps|decision_support|operator_summary|family_key|historical_examples|close_when|note_guidance|caution_signals)`"
)
SOURCE_HEADER_PATTERNS = [
    re.compile(r"(?i)original service case:\s*[^.]+"),
    re.compile(r"(?i)opened in source system:\s*[^.]+"),
    re.compile(r"(?i)channel:\s*[^.]+"),
    re.compile(r"(?i)category:\s*[^.]+"),
    re.compile(r"(?i)subcategory:\s*[^.]+"),
    re.compile(r"(?i)type:\s*[^.]+"),
    re.compile(r"(?i)impact:\s*[^.]+"),
    re.compile(r"(?i)branch/location:\s*[^.]+"),
    re.compile(r"(?i)imported request context:\s*"),
]


def slug(text: str) -> str:
    return re.sub(r"[^a-z0-9-]+", "-", text.lower()).strip("-")


def title_case_from_slug(value: str) -> str:
    return " ".join(part.capitalize() for part in value.split("-") if part)


def ensure_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)


def write(path: Path, content: str) -> None:
    path.write_text(content.rstrip() + "\n", encoding="utf-8")


def safe_list_generated_files(analysis_dir: Path) -> list[Path]:
    allowed_suffixes = {".json", ".jsonl", ".md", ".txt", ".npy"}
    files = []
    for path in sorted(analysis_dir.iterdir()):
        if path.is_file() and path.suffix in allowed_suffixes:
            files.append(path)
    return files


def shorten(text: str, limit: int = 220) -> str:
    text = re.sub(r"\s+", " ", str(text or "").strip())
    return text if len(text) <= limit else text[: limit - 3].rstrip() + "..."


def sanitize_skill_text(text: str, limit: int = 220) -> str:
    value = re.sub(r"\s+", " ", str(text or "").strip())
    if not value:
        return value
    for pattern in SOURCE_HEADER_PATTERNS:
        value = pattern.sub(" ", value)
    value = FIELD_NAME_PATTERN.sub("", value)
    value = LEAKY_TERMS.sub("", value)
    value = re.sub(r"\b\d{2}\.\d{2}\.\d{4}\b", " ", value)
    value = re.sub(r"\b\d{2}\.\d{4}\b", " ", value)
    value = re.sub(r"(?i)historically observed", "established", value)
    value = re.sub(r"(?i)evidence shows", "repeatedly seen is", value)
    value = re.sub(r"(?i)operators treat this as", "diese Fälle laufen als", value)
    value = re.sub(r"(?i)ticket family is", "diese Familie ist", value)
    value = value.replace("`", "")
    value = re.sub(r"\s+", " ", value).strip(" .;,-")
    return shorten(value, limit)


def sanitize_list(items: list[str], limit: int, item_limit: int) -> list[str]:
    result: list[str] = []
    for item in items:
        cleaned = sanitize_skill_text(item, item_limit)
        if cleaned and cleaned not in result:
            result.append(cleaned)
        if len(result) >= limit:
            break
    return result


def naturalize_goal_text(text: str) -> str:
    return sanitize_skill_text(text, 260) or "work in the desk's established operating style"


def family_scope_label(family_key: str) -> str:
    parts = [sanitize_skill_text(part, 80) for part in str(family_key or "").split("::")]
    parts = [part for part in parts if part]
    if not parts:
        return "diese Ticketfamilie"
    return " / ".join(parts)


def lower_sentence_start(text: str) -> str:
    cleaned = sanitize_skill_text(text, 180)
    if not cleaned:
        return cleaned
    return cleaned[0].lower() + cleaned[1:] if len(cleaned) > 1 else cleaned.lower()


def desk_voice_summary(playbook: dict[str, Any]) -> str:
    decision = playbook.get("decision_support") or {}
    mode = sanitize_skill_text(decision.get("mode", ""), 40)
    family_scope = family_scope_label(playbook.get("family_key", ""))
    close_when = sanitize_skill_text(decision.get("close_when", ""), 160)
    intro_map = {
        "access_change": (
            f"Diese Fälle laufen als {family_scope}. "
            "Meist geht es um Sperrung, Entsperrung oder Pflege einer Kennung. "
            "Zuerst werden betroffene Kennung und gewünschte Änderung sauber abgeglichen. "
            "Danach wird die konkrete Pflegeaktion knapp festgehalten."
        ),
        "monitoring_signal": (
            f"Diese Fälle laufen als {family_scope}. "
            "Meist geht es um eine Störung, Systemmeldung oder Gesundheitsprüfung. "
            "Zuerst werden betroffene Komponente und Fehlersignal bestätigt. "
            "Danach wird die Prüfung, Gegenmaßnahme oder Wiederherstellung knapp dokumentiert."
        ),
        "integration_issue": (
            f"Diese Fälle laufen als {family_scope}. "
            "Meist geht es um einen fehlerhaften Import, Transfer oder Hintergrundlauf. "
            "Zuerst wird der betroffene Lauf oder die Schnittstelle eingegrenzt. "
            "Danach wird der Retry oder der nächste erfolgreiche Lauf festgehalten."
        ),
        "notification_or_distribution": (
            f"Diese Fälle laufen als {family_scope}. "
            "Meist muss geklärt werden, ob aus der Meldung nur Information oder eine Folgeaktion entsteht. "
            "Zuerst wird der operative Kern der Nachricht isoliert. "
            "Danach wird nur die eigentliche Folgeaktion dokumentiert."
        ),
    }
    summary = intro_map.get(
        mode,
        f"Diese Fälle laufen als {family_scope}. Zuerst wird der Vorgang eingegrenzt. Danach wird der konkrete Arbeitsschritt knapp dokumentiert.",
    )
    if close_when:
        summary += f" Geschlossen wird, wenn {lower_sentence_start(close_when)}."
    return sanitize_skill_text(summary, 260)


def note_style_anchors(playbooks: list[dict[str, Any]], limit: int = 4) -> list[str]:
    anchors: list[str] = []
    for playbook in playbooks:
        decision = playbook.get("decision_support") or {}
        mode = sanitize_skill_text(decision.get("mode", ""), 40)
        candidates = decision.get("note_anchors") or []
        if not candidates:
            if mode == "access_change":
                candidates = [
                    "Betroffene Kennung zuerst nennen.",
                    "Dann die konkrete Sperr-, Entsperr- oder Pflegeaktion notieren.",
                    "Zum Schluss sagen, ob ein Retry möglich ist.",
                ]
            elif mode == "monitoring_signal":
                candidates = [
                    "Komponente oder Host zuerst nennen.",
                    "Dann Prüfung, Restart oder Gegenmaßnahme festhalten.",
                    "Zum Schluss den aktuellen Zustand bestätigen.",
                ]
            elif mode == "integration_issue":
                candidates = [
                    "Betroffene Schnittstelle oder den Lauf nennen.",
                    "Dann den Retry oder die Prüfung kurz festhalten.",
                    "Zum Schluss den sichtbaren Folgezustand nennen.",
                ]
            else:
                candidates = [
                    "Nur die operative Folgeaktion notieren.",
                    "Betroffenes Objekt und Ergebnis in einem kurzen Satz festhalten.",
                ]
        for candidate in candidates:
            cleaned = sanitize_skill_text(candidate, 180)
            if cleaned and cleaned not in anchors:
                anchors.append(cleaned)
            if len(anchors) >= limit:
                return anchors
    return anchors


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def top_family_playbooks(analysis_dir: Path, limit: int = 6) -> list[dict[str, Any]]:
    path = analysis_dir / "family_playbooks.json"
    if not path.exists():
        return []
    items = load_json(path)
    if not isinstance(items, list):
        return []
    ranked = sorted(items, key=lambda item: int(item.get("rank") or 999999))
    return ranked[:limit]


def top_note_examples(playbooks: list[dict[str, Any]], limit: int = 4) -> list[str]:
    examples: list[str] = []
    for playbook in playbooks:
        note_style = playbook.get("note_style") or {}
        for example in note_style.get("manual_note_examples", []):
            cleaned = sanitize_skill_text(example, 180)
            if cleaned and cleaned not in examples:
                examples.append(cleaned)
            if len(examples) >= limit:
                return examples
    return examples


def top_cautions(playbooks: list[dict[str, Any]], limit: int = 6) -> list[str]:
    cautions: list[str] = []
    for playbook in playbooks:
        decision = playbook.get("decision_support") or {}
        for caution in decision.get("caution_signals", []):
            cleaned = sanitize_skill_text(caution, 160)
            if cleaned and cleaned not in cautions:
                cautions.append(cleaned)
            if len(cautions) >= limit:
                return cautions
    return cautions


def top_queries(playbooks: list[dict[str, Any]], limit: int = 6) -> list[str]:
    queries: list[str] = []
    for playbook in playbooks:
        decision = playbook.get("decision_support") or {}
        for query in decision.get("triage_focus", []):
            cleaned = sanitize_skill_text(query, 120)
            if cleaned and cleaned not in queries:
                queries.append(cleaned)
            if len(queries) >= limit:
                return queries
    return queries


def build_family_highlights(playbooks: list[dict[str, Any]]) -> str:
    lines = ["# Family Highlights", ""]
    if not playbooks:
        lines.append("No family playbooks were promoted into this generated skill.")
        return "\n".join(lines) + "\n"
    for playbook in playbooks:
        decision = playbook.get("decision_support") or {}
        examples = (playbook.get("historical_examples") or {}).get("canonical") or []
        lines.append(f"## {playbook['family_key']}")
        lines.append("")
        lines.append(f"- Desk pattern: {desk_voice_summary(playbook)}")
        triage = decision.get("triage_focus") or []
        if triage:
            lines.append(f"- First checks: {'; '.join(sanitize_list(triage[:4], 4, 120))}")
        steps = decision.get("handling_steps") or []
        if steps:
            lines.append(f"- Typical handling: {'; '.join(sanitize_list(steps[:4], 4, 180))}")
        if decision.get("close_when"):
            lines.append(f"- Close when: {sanitize_skill_text(decision['close_when'], 180)}")
        if examples:
            lines.append(f"- Historical anchors: {', '.join(example['ticket_id'] for example in examples[:3])}")
        lines.append("")
    return "\n".join(lines) + "\n"


def build_frontmatter(skill_name: str, description: str) -> str:
    return f"---\nname: {skill_name}\ndescription: {description}\n---\n"


def build_skill_body(
    display_name: str,
    archetype: str,
    dataset_label: str,
    goal: str,
    generated_files: list[str],
    query_command: str | None,
    playbooks: list[dict[str, Any]],
) -> str:
    natural_goal = naturalize_goal_text(goal)
    archetype_text = {
        "operating-model": "a reusable operating-model skill from historical evidence",
        "lookup-reference": "a reusable lookup/reference skill from a durable catalog or dataset",
        "workflow": "a reusable workflow skill from repeated process evidence",
        "policy-gate": "a reusable policy and approval skill from structured decision evidence",
    }[archetype]
    lines = [
        f"# {display_name}",
        "",
        "## Overview",
        "",
        f"Use this skill when CTOX should {natural_goal}.",
        "",
        f"This generated skill is based on `{dataset_label}` and is shaped as {archetype_text}.",
        "",
    ]
    if archetype == "operating-model" and playbooks:
        note_examples = note_style_anchors(playbooks)
        cautions = top_cautions(playbooks)
        first_checks = top_queries(playbooks)
        lines.extend(
            [
                "## How To Handle A New Ticket",
                "",
                "1. Read the new ticket and identify the key systems, users, locations, or alerts it mentions.",
                "2. Use the helper entry point to retrieve the most likely historical family and examples.",
                "3. Start from that family's first checks, then follow its usual handling path instead of inventing a new process.",
                "4. Mirror the desk's short internal note style and closure logic rather than writing generic AI commentary.",
                "5. If the match is weak or the matched family already contains warning signs about ambiguity, stop and escalate instead of forcing the wrong family.",
                "",
                "## What To Do First",
                "",
                "Always prefer the strongest historical family with real examples over generic category guessing.",
                "Use the first matching family to decide:",
                "",
                "- what to check first",
                "- what usually happens next",
                "- how operators in this desk phrase internal notes",
                "- what closure looks like here",
                "",
            ]
        )
        if first_checks:
            lines.extend(
                [
                    "Typical first checks that appeared repeatedly in the promoted families:",
                    "",
                ]
            )
            lines.extend(f"- {item}" for item in first_checks[:5])
            lines.append("")
        lines.extend(["## Priority Families", ""])
        for playbook in playbooks[:5]:
            decision = playbook.get("decision_support") or {}
            examples = (playbook.get("historical_examples") or {}).get("canonical") or []
            lines.append(f"### {playbook['family_key']}")
            lines.append("")
            lines.append(f"- Desk pattern: {desk_voice_summary(playbook)}")
            triage = decision.get("triage_focus") or []
            if triage:
                lines.append(f"- Check first: {'; '.join(sanitize_list(triage[:4], 4, 120))}")
            steps = decision.get("handling_steps") or []
            if steps:
                lines.append(f"- Usually next: {'; '.join(sanitize_list(steps[:4], 4, 180))}")
            if decision.get("close_when"):
                lines.append(f"- Close when: {sanitize_skill_text(decision['close_when'], 180)}")
            if examples:
                lines.append(
                    f"- Good examples: {', '.join(example['ticket_id'] + ' ' + sanitize_skill_text(example['title'], 100) for example in examples[:2])}"
                )
            lines.append("")
        lines.extend(
            [
                "## How To Write Internal Notes",
                "",
                "Write short, concrete operational notes.",
                "Capture the action taken, the affected object, and the verified outcome.",
                "Use the desk's natural working language, not extracted raw source fragments.",
            ]
        )
        if note_examples:
            lines.extend(["", "Note style anchors:"])
            lines.extend(f"- {example}" for example in note_examples)
        lines.extend(["", "## When To Escalate", ""])
        lines.extend(
            [
                "- No family matches strongly enough to trust the handling pattern.",
                "- The strongest family still leaves identity, host, or service ambiguity.",
                "- The required action depends on rights, approvals, or systems that are not available.",
                "- Historical evidence is too sparse to justify autonomous handling.",
                "",
                "## Common Failure Modes",
                "",
            ]
        )
        if cautions:
            lines.extend(f"- {caution}" for caution in cautions)
        else:
            lines.append("- Forcing a family match without real historical support.")
        lines.extend(["", "## References"])
    else:
        lines.extend(["## Workflow"])
    lines.extend(
        [
        "",
        "1. Read `references/source-analysis.md` first.",
        "2. Read the generated evidence under `references/generated/` that matches the current task.",
        "3. Reuse the promoted operating patterns, references, or policy boundaries instead of rediscovering them from scratch.",
        "4. Keep output in the language of the target work domain, not in extraction or tooling language.",
        "",
        "## Generated References",
        "",
        "- `references/source-analysis.md`",
        "- `references/generated/`",
        "- `references/family-highlights.md`",
    ])
    if query_command:
        lines.extend(["", "## Helper Entry Point", "", "Use this helper when fast retrieval from the promoted evidence is needed:", "", "```bash", query_command, "```"])
    if generated_files:
        lines.extend(["", "## Promoted Artifacts", ""])
        lines.extend(f"- `references/generated/{name}`" for name in generated_files)
    lines.extend(
        [
            "",
            "## Success",
            "",
            f"The skill succeeds when CTOX can {natural_goal} by reusing the promoted evidence instead of rebuilding the same understanding from the raw dataset each turn.",
        ]
    )
    return "\n".join(lines) + "\n"


def build_source_analysis(dataset_label: str, archetype: str, goal: str, generated_files: list[str]) -> str:
    lines = [
        "# Source Analysis",
        "",
        f"- Dataset label: `{dataset_label}`",
        f"- Generated skill archetype: `{archetype}`",
        f"- Operating goal: {goal}",
        "",
        "## Promoted Files",
        "",
    ]
    if generated_files:
        lines.extend(f"- `{name}`" for name in generated_files)
    else:
        lines.append("- No analysis bundle was copied; add promoted references manually.")
    lines.append("")
    lines.append("Only durable, reusable artifacts should be treated as canonical references for the generated skill.")
    return "\n".join(lines) + "\n"


def build_openai_yaml(display_name: str, short_description: str, default_prompt: str) -> str:
    return (
        "interface:\n"
        f'  display_name: "{display_name}"\n'
        f'  short_description: "{short_description}"\n'
        f'  default_prompt: "{default_prompt}"\n'
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Create a reusable skill from a dataset-derived analysis bundle.")
    parser.add_argument("--skill-name", required=True)
    parser.add_argument("--skill-path", required=True)
    parser.add_argument("--archetype", required=True, choices=sorted(VALID_ARCHETYPES))
    parser.add_argument("--dataset-label", required=True)
    parser.add_argument("--goal", required=True)
    parser.add_argument("--analysis-dir")
    parser.add_argument("--query-command")
    parser.add_argument("--display-name")
    parser.add_argument("--short-description")
    parser.add_argument("--default-prompt")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    skill_name = slug(args.skill_name)
    if not skill_name:
        raise SystemExit("skill name slugged to empty value")
    display_name = args.display_name or title_case_from_slug(skill_name)
    natural_goal = naturalize_goal_text(args.goal)
    short_description = args.short_description or f"Use {display_name} for {natural_goal}"
    default_prompt = args.default_prompt or f"Use this skill to {natural_goal}."

    skill_dir = Path(args.skill_path) / skill_name
    ensure_dir(skill_dir)
    ensure_dir(skill_dir / "agents")
    ensure_dir(skill_dir / "references")
    ensure_dir(skill_dir / "references" / "generated")

    generated_files: list[str] = []
    playbooks: list[dict[str, Any]] = []
    if args.analysis_dir:
        analysis_dir = Path(args.analysis_dir)
        if not analysis_dir.exists():
            raise SystemExit(f"analysis directory not found: {analysis_dir}")
        playbooks = top_family_playbooks(analysis_dir)
        for source in safe_list_generated_files(analysis_dir):
            target = skill_dir / "references" / "generated" / source.name
            shutil.copy2(source, target)
            generated_files.append(source.name)

    description = (
        f"Use when CTOX should {natural_goal} by reusing the promoted evidence from `{args.dataset_label}` "
        f"instead of rediscovering the same structure from raw data each turn."
    )
    skill_content = build_frontmatter(skill_name, description) + "\n" + build_skill_body(
        display_name,
        args.archetype,
        args.dataset_label,
        args.goal,
        generated_files,
        args.query_command,
        playbooks,
    )

    write(skill_dir / "SKILL.md", skill_content)
    write(skill_dir / "references" / "source-analysis.md", build_source_analysis(args.dataset_label, args.archetype, args.goal, generated_files))
    write(skill_dir / "references" / "family-highlights.md", build_family_highlights(playbooks))
    write(skill_dir / "agents" / "openai.yaml", build_openai_yaml(display_name, short_description, default_prompt))

    print(
        json.dumps(
            {
                "skill_dir": str(skill_dir),
                "skill_name": skill_name,
                "archetype": args.archetype,
                "generated_reference_count": len(generated_files),
                "generated_references": generated_files,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
