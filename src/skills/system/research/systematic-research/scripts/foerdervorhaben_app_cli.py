#!/usr/bin/env python3
"""CLI port of the Foerdervorhaben-Agent app workflow.

This is intentionally a narrow, deterministic helper for the
`project_description` flavor. It does not generate prose itself. It exposes the
same app-shaped stages the HTML app used: workspace snapshot, asset lookup,
block writer prompt, revision prompt, narrative-flow prompt, and release lint.
The harness LLM must use these stage artifacts instead of the generic
deep-research writer path.
"""

from __future__ import annotations

import argparse
import datetime as _dt
import html
import json
import os
import re
import sqlite3
import subprocess
import sys
import uuid
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


APP_MANAGER_INSTRUCTIONS = [
    "Du orchestrierst die Erstellung einer Foerdervorhabenbeschreibung.",
    "Schreibe keine Dokumentbloecke direkt im Manager; nutze die CLI-Stages writer-prompt, revision-prompt und flow-review-prompt.",
    "Pfad: workspace_snapshot -> asset_lookup -> reference-fit contract -> writer-prompt/revision-prompt -> block-stage/block-apply -> completeness_check -> character_budget_check -> release_guard_check -> narrative_flow_check -> deliverable_quality -> reference-fit lint.",
    "Arbeite in kleinen Paketen von hoechstens 6 Bloecken.",
    "Wenn Review-Feedback vorliegt, bearbeite betroffene Bloecke zuerst mit revision-prompt.",
    "Stelle Rueckfragen, wenn Ist-Zustand, Zielbild, Perspektive, Kennzahlen oder zentrale Sachverhalte sonst spekulativ wuerden.",
    "Recherche ist optional und nur fuer oeffentliche belastbare Fakten gedacht; sie darf im finalen Dokument nicht als Quellenapparat erscheinen.",
    "Beende nur, wenn Vollstaendigkeit, Zeichenbudget, Release Guard, Narrative Flow und Deliverable Quality ready sind.",
]

WRITER_INSTRUCTIONS = [
    "Du bist der Block Writer Skill fuer eine Foerdervorhabenbeschreibung.",
    "Erzeuge nur die angeforderten Blockinhalte.",
    "Nutze ausschliesslich bereitgestellte Fakten, Nutzerantworten, Review-Feedback und optionale oeffentliche Recherche.",
    "Schreibe wie ein starkes Unternehmensdossier: unternehmerisch-professionell, konkret, nah an der operativen Realitaet, nicht behoerdlich-kuehl.",
    "Schreibe aus interner Feststellungsperspektive oder ruhiger Wir-Perspektive; nie aus Gutachter-, Beobachter-, Akten- oder Prueferperspektive.",
    "Research ist Arbeitsmaterial. Keine sichtbaren Quellen, URLs, DOI, Evidence-IDs, Tool-/Workspace-/QA-Begriffe und keine Beleglueckenkommentare.",
    "Jeder wichtige Absatz braucht einen konkreten Unternehmens- oder Vorhabensanker und danach eine kurze Einordnung.",
    "Probleme klar benennen, aber das Unternehmen nicht unnoetig schwach oder passiv darstellen.",
    "Wenn Fakten fehlen, keine Unsicherheit ausschreiben; Rueckfrage ausloesen oder Punkt weglassen.",
    "Blockueberschrift nicht wiederholen; gib nur reife Unterlagenprosa in Markdown zurueck.",
]

REVISION_INSTRUCTIONS = [
    "Du bist der Revision Skill fuer die Foerdervorhabenbeschreibung.",
    "Ueberarbeite nur die angeforderten Bloecke und erhalte belastbare Sachinhalte.",
    "Entferne Gutachterton, Aktenstil, Meta-Kommentare, Quellen-/Rechercheprosa, Platzhalter und generische Foerderformeln.",
    "Verbessere roten Faden, Lesbarkeit, Abschnittsuebergaenge und konkrete Unternehmensanker.",
    "Bei Formrevisionen keine neue Projektlogik und keine neuen Fakten einfuehren.",
    "Wenn eine Aussage ohne Nutzerklaerung kippen wuerde, gib Rueckfragen statt geratenem Text.",
]

FLOW_REVIEW_INSTRUCTIONS = [
    "Du bist der Narrative Flow Skill fuer eine Foerdervorhabenbeschreibung.",
    "Pruefe abschnittsuebergreifend: Unternehmensbild -> Entwicklungsdruck -> Vorhaben -> Problem -> Zielbild -> Umsetzung -> Kosten/Zeitraum -> Nutzen.",
    "Markiere harte Neustarts, redundante Wiedereinfuehrungen, Gutachterperspektive, generische Foerderprosa und sichtbare Research-Mechanik.",
    "Wenn Revision noetig ist, nenne konkrete instance_ids und praezise Ziele.",
]

FORBIDDEN_PATTERNS: list[tuple[str, str]] = [
    ("APPBASELINE-GUTACHTERTON", r"\bf[üu]r diese f[öo]rderunterlage gen[üu]gt\b"),
    ("APPBASELINE-REGISTERMATERIAL", r"\bmehr registermaterial\b"),
    ("APPBASELINE-PLAUSIBILITAETSREGISTER", r"\bplausibilit[aä]t des vorhabens\b"),
    ("APPBASELINE-LIZENZKONSTRUKTION", r"\blizenzkonstruktion\b"),
    ("APPBASELINE-RECHTLICHER-RAHMEN", r"\brechtlicher rahmen\b.*\bprojektverantwortung\b"),
    ("APPBASELINE-EXTERNAL-ASSESSMENT", r"\btritt als klar zuordenbare\b"),
    ("APPBASELINE-GESELLSCHAFTSTRAEGER", r"\btechnologie- und servicetr[aä]ger\b"),
    ("APPBASELINE-NACHVOLLZIEHBARER-RAHMEN", r"\b[öo]ffentlich nachvollziehbarer gesellschaftsrechtlicher rahmen\b"),
    ("APPBASELINE-RESEARCH-PROSE", r"\b(auf basis der recherche|die recherche zeigt|die quellen zeigen|recherchebasis)\b"),
    ("APPBASELINE-CONTEXT-PROSE", r"\b(nach dem vorliegenden kontext|soweit beigef[üu]gt|nicht gesondert belegt|liegt derzeit nicht vor)\b"),
    ("APPBASELINE-INTERNAL-TOOLING", r"\b(ctox|workspace|evidence|fact-transfer|asset-pack|flavor|adaptervertrag|qa|run_)\b"),
    ("APPBASELINE-DUP-FIGURE-LABEL", r"\babbildung\s+abbildung\b"),
    ("APPBASELINE-FOERDER-EXPLAINER", r"\bf[üu]r das f[öo]rdervorhaben ist\b"),
    ("APPBASELINE-ANTRAGSTELLERIN-OPENING", r"\bdie antragstellerin ist\b"),
]

REFERENCE_FIT_DEFAULT_MIN_FIGURES = 5
REFERENCE_FIT_DEFAULT_MIN_TABLES = 2


def repo_root_from(start: Path) -> Path:
    cur = start.resolve()
    for path in [cur, *cur.parents]:
        if (path / "Cargo.toml").exists() and (path / "skills/system/research/systematic-research").exists():
            return path
    return cur


@dataclass
class Ctx:
    root: Path
    run_id: str

    @property
    def db(self) -> Path:
        return self.root / "runtime" / "ctox.sqlite3"

    @property
    def out_dir(self) -> Path:
        return self.root / "runtime" / "foerdervorhaben_app_cli" / self.run_id

    @property
    def asset_pack_path(self) -> Path:
        return self.root / "skills/system/research/systematic-research/references/foerdervorhaben_agent_asset_pack.json"

    @property
    def agent_contract_path(self) -> Path:
        return self.root / "runtime/report_project_description_agent" / self.run_id / "foerdervorhaben-agent-contract.json"


def db_rows(ctx: Ctx, sql: str, params: tuple[Any, ...] = ()) -> list[dict[str, Any]]:
    if not ctx.db.exists():
        raise SystemExit(f"missing CTOX DB: {ctx.db}")
    conn = sqlite3.connect(ctx.db)
    conn.row_factory = sqlite3.Row
    try:
        return [dict(row) for row in conn.execute(sql, params).fetchall()]
    finally:
        conn.close()


def record_stage(ctx: Ctx, stage: str, payload: dict[str, Any] | None = None) -> None:
    if not ctx.run_id or not ctx.db.exists():
        return
    conn = sqlite3.connect(ctx.db)
    try:
        conn.execute(
            """INSERT INTO report_provenance (
                   prov_id, run_id, kind, occurred_at, instance_id, skill_run_id,
                   research_id, payload_json
               ) VALUES (?, ?, ?, ?, NULL, NULL, NULL, ?)""",
            (
                f"prov_{uuid.uuid4().hex}",
                ctx.run_id,
                f"project_description_app_cli_{stage}",
                _dt.datetime.now(_dt.timezone.utc).isoformat(),
                json.dumps(payload or {}, ensure_ascii=False),
            ),
        )
        conn.commit()
    finally:
        conn.close()


def run_row(ctx: Ctx) -> dict[str, Any]:
    rows = db_rows(
        ctx,
        "SELECT run_id, report_type_id, domain_profile_id, depth_profile_id, language, status, raw_topic FROM report_runs WHERE run_id=?",
        (ctx.run_id,),
    )
    if not rows:
        raise SystemExit(f"unknown run_id: {ctx.run_id}")
    if rows[0]["report_type_id"] != "project_description":
        raise SystemExit(f"run is {rows[0]['report_type_id']}, expected project_description")
    return rows[0]


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def ensure_contract(ctx: Ctx) -> dict[str, Any]:
    if not ctx.agent_contract_path.exists():
        subprocess.run(
            [os.environ.get("CTOX_BIN", "ctox"), "report", "project-description-agent-brief", "--run-id", ctx.run_id],
            cwd=ctx.root,
            check=True,
        )
    return load_json(ctx.agent_contract_path)


def committed_blocks(ctx: Ctx) -> list[dict[str, Any]]:
    return db_rows(
        ctx,
        "SELECT instance_id, doc_id, block_id, title, ord, markdown, committed_at FROM report_blocks WHERE run_id=? ORDER BY ord ASC, committed_at ASC",
        (ctx.run_id,),
    )


def pending_blocks(ctx: Ctx) -> list[dict[str, Any]]:
    return db_rows(
        ctx,
        "SELECT instance_id, doc_id, block_id, title, ord, markdown, committed_at FROM report_pending_blocks WHERE run_id=? ORDER BY ord ASC, committed_at ASC",
        (ctx.run_id,),
    )


def evidence(ctx: Ctx) -> list[dict[str, Any]]:
    rows = db_rows(
        ctx,
        "SELECT evidence_id, title, year, publisher, url_canonical, url_full_text, substr(coalesce(abstract_md, snippet_md, full_text_md, ''), 1, 1200) AS excerpt FROM report_evidence_register WHERE run_id=? ORDER BY created_at ASC",
        (ctx.run_id,),
    )
    return rows


def review_feedback(ctx: Ctx) -> list[dict[str, Any]]:
    return db_rows(
        ctx,
        "SELECT feedback_id, instance_id, source_file, form_only, body, imported_at FROM report_review_feedback WHERE run_id=? ORDER BY imported_at ASC",
        (ctx.run_id,),
    )


def contract_blocks(contract: dict[str, Any], instance_ids: list[str] | None = None) -> list[dict[str, Any]]:
    blocks = list(contract.get("block_contract") or [])
    if instance_ids:
        wanted = set(instance_ids)
        blocks = [b for b in blocks if b.get("instance_id") in wanted]
    return blocks


def make_snapshot(ctx: Ctx) -> dict[str, Any]:
    run = run_row(ctx)
    contract = ensure_contract(ctx)
    blocks = committed_blocks(ctx)
    pending = pending_blocks(ctx)
    ev = evidence(ctx)
    feedback = review_feedback(ctx)
    return {
        "run": run,
        "character_count_committed": sum(len(b.get("markdown") or "") for b in blocks),
        "committed_blocks": blocks,
        "pending_blocks": pending,
        "evidence": ev,
        "review_feedback": feedback,
        "required_block_contract": contract_blocks(contract),
        "app_pipeline": [
            "workspace_snapshot",
            "asset_lookup",
            "writer_prompt",
            "block_stage",
            "block_apply",
            "revision_prompt",
            "flow_review_prompt",
            "release_lint",
            "project_description_sync",
            "ctox_checks",
            "render_docx",
        ],
    }


def emit_json(data: Any, out: str | None = None) -> None:
    text = json.dumps(data, ensure_ascii=False, indent=2)
    if out:
        Path(out).write_text(text + "\n", encoding="utf-8")
    else:
        print(text)


def write_init(ctx: Ctx, out: str | None = None) -> None:
    run_row(ctx)
    contract = ensure_contract(ctx)
    ctx.out_dir.mkdir(parents=True, exist_ok=True)
    (ctx.out_dir / "manager_instructions.md").write_text("\n".join(f"- {x}" for x in APP_MANAGER_INSTRUCTIONS) + "\n", encoding="utf-8")
    (ctx.out_dir / "writer_instructions.md").write_text("\n".join(f"- {x}" for x in WRITER_INSTRUCTIONS) + "\n", encoding="utf-8")
    (ctx.out_dir / "revision_instructions.md").write_text("\n".join(f"- {x}" for x in REVISION_INSTRUCTIONS) + "\n", encoding="utf-8")
    (ctx.out_dir / "flow_review_instructions.md").write_text("\n".join(f"- {x}" for x in FLOW_REVIEW_INSTRUCTIONS) + "\n", encoding="utf-8")
    forbidden = [{"lint_id": lint_id, "pattern": pattern} for lint_id, pattern in FORBIDDEN_PATTERNS]
    (ctx.out_dir / "forbidden_patterns.json").write_text(json.dumps(forbidden, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    payload = {
        "ok": True,
        "run_id": ctx.run_id,
        "cli_workspace": str(ctx.out_dir),
        "contract": str(ctx.agent_contract_path),
        "block_count": len(contract_blocks(contract)),
        "next": [
            f"python3 {Path(__file__).name} workspace-snapshot --run-id {ctx.run_id}",
            f"python3 {Path(__file__).name} asset-lookup --run-id {ctx.run_id}",
            f"python3 {Path(__file__).name} writer-prompt --run-id {ctx.run_id} --instance-id <ID> --out /tmp/block_prompt.md",
        ],
    }
    emit_json(payload, out)
    record_stage(ctx, "init", {"cli_workspace": str(ctx.out_dir)})


def asset_lookup(ctx: Ctx, instance_ids: list[str], out: str | None = None) -> None:
    run = run_row(ctx)
    contract = ensure_contract(ctx)
    asset_pack = load_json(ctx.asset_pack_path)
    payload = {
        "run": run,
        "style_guidance": asset_pack.get("style_guidance", {}),
        "style_rules": asset_pack.get("style_rules", {}),
        "style_profiles": asset_pack.get("style_profiles", []),
        "reference_patterns": asset_pack.get("reference_patterns", []),
        "selected_blocks": contract_blocks(contract, instance_ids or None),
    }
    emit_json(payload, out)
    record_stage(ctx, "asset_lookup", {"instance_ids": instance_ids})


def prompt_payload(ctx: Ctx, instance_ids: list[str], mode: str) -> dict[str, Any]:
    contract = ensure_contract(ctx)
    snapshot = make_snapshot(ctx)
    selected = contract_blocks(contract, instance_ids)
    if not selected:
        raise SystemExit("no selected block matched --instance-id")
    existing_by_id = {b["instance_id"]: b for b in snapshot["committed_blocks"]}
    return {
        "mode": mode,
        "instructions": {
            "write": WRITER_INSTRUCTIONS,
            "revision": REVISION_INSTRUCTIONS,
            "flow_review": FLOW_REVIEW_INSTRUCTIONS,
        }[mode],
        "package_context": snapshot["run"],
        "selected_blocks": selected,
        "existing_blocks": [existing_by_id[i] for i in instance_ids if i in existing_by_id],
        "document_flow": [
            {
                "instance_id": b.get("instance_id"),
                "title": b.get("title"),
                "order": b.get("order"),
                "status": "committed" if b.get("instance_id") in existing_by_id else "missing",
            }
            for b in contract_blocks(contract)
        ],
        "review_feedback": snapshot["review_feedback"],
        "evidence_working_material": snapshot["evidence"],
        "forbidden_output_patterns": [{"lint_id": x, "pattern": y} for x, y in FORBIDDEN_PATTERNS],
    }


def write_prompt(ctx: Ctx, instance_ids: list[str], mode: str, out: str | None = None) -> None:
    payload = prompt_payload(ctx, instance_ids, mode)
    prompt = [
        "# Foerdervorhaben-App CLI Stage",
        "",
        "Use this as the complete stage input. Produce only the requested block prose or review verdict.",
        "",
        "```json",
        json.dumps(payload, ensure_ascii=False, indent=2),
        "```",
        "",
        "Output contract:",
        "- For write/revision: return Markdown prose per selected block, grouped by instance_id.",
        "- Do not include sources, URLs, evidence IDs, tool names, QA language or meta commentary.",
        "- If blocked, return up to three concrete user questions instead of prose.",
    ]
    text = "\n".join(prompt) + "\n"
    if out:
        Path(out).write_text(text, encoding="utf-8")
    else:
        print(text)
    record_stage(ctx, mode.replace("_", "-"), {"instance_ids": instance_ids, "out": out})


def normalize_text(text: str) -> str:
    return re.sub(r"\s+", " ", text.replace("\xa0", " ")).strip()


def lint_text(text: str) -> list[dict[str, str]]:
    lower = normalize_text(text).lower()
    issues = []
    for lint_id, pattern in FORBIDDEN_PATTERNS:
        if re.search(pattern, lower, flags=re.IGNORECASE):
            issues.append(
                {
                    "lint_id": lint_id,
                    "reason": "Finale Projektbeschreibung enthaelt App-Baseline-widrige Meta-/Gutachter-/Tool-Sprache.",
                    "pattern": pattern,
                }
            )
    return issues


def docx_text(path: Path) -> str:
    with zipfile.ZipFile(path) as zf:
        xml = zf.read("word/document.xml").decode("utf-8", "ignore")
    chunks = re.findall(r"<w:t[^>]*>(.*?)</w:t>", xml)
    return html.unescape(" ".join(chunks))


def docx_profile(path: Path) -> dict[str, Any]:
    with zipfile.ZipFile(path) as zf:
        names = zf.namelist()
        xml = zf.read("word/document.xml").decode("utf-8", "ignore")
        comments = 0
        if "word/comments.xml" in names:
            comments_xml = zf.read("word/comments.xml").decode("utf-8", "ignore")
            comments = comments_xml.count("<w:comment")
    table_xmls = re.findall(r"<w:tbl\b.*?</w:tbl>", xml, flags=re.DOTALL)
    table_texts = []
    for table_xml in table_xmls:
        chunks = re.findall(r"<w:t[^>]*>(.*?)</w:t>", table_xml)
        table_texts.append(normalize_text(html.unescape(" ".join(chunks))))
    chunks = re.findall(r"<w:t[^>]*>(.*?)</w:t>", xml)
    text = normalize_text(html.unescape(" ".join(chunks)))
    media = [name for name in names if name.startswith("word/media/")]
    return {
        "path": str(path),
        "text": text,
        "chars": len(text),
        "tables": len(table_xmls),
        "table_texts": table_texts,
        "images": len(media),
        "comments": comments,
    }


def reference_dirs(root: Path, explicit: list[str] | None = None) -> list[Path]:
    dirs = [Path(p) for p in explicit or []]
    for candidate in [
        root / "reference_inputs",
        Path("/home/ubuntu/reference_inputs"),
        Path("/Users/michaelwelsch/Downloads/OneDrive_1_9"),
    ]:
        if candidate.exists():
            dirs.append(candidate)
    seen: set[Path] = set()
    out = []
    for d in dirs:
        d = d.resolve()
        if d not in seen and d.exists():
            out.append(d)
            seen.add(d)
    return out


def project_reference_fit(docx: Path, refs: list[Path], min_figures: int, min_tables: int) -> dict[str, Any]:
    profile = docx_profile(docx)
    text_lower = profile["text"].lower()
    issues: list[dict[str, Any]] = []
    ref_profiles = []
    for ref_dir in refs:
        for ref in sorted(ref_dir.glob("*.docx")):
            try:
                rp = docx_profile(ref)
            except Exception:
                continue
            ref_profiles.append(
                {
                    "path": str(ref),
                    "chars": rp["chars"],
                    "tables": rp["tables"],
                    "images": rp["images"],
                    "comments": rp["comments"],
                }
            )
    if profile["images"] < min_figures:
        issues.append(
            {
                "lint_id": "PROJECT-FIT-IMAGE-DENSITY",
                "reason": f"Nur {profile['images']} eingebettete Abbildung(en); echte Projektbeschreibungen brauchen visuelle Unternehmens-/Produkt-/Projektfuehrung.",
                "goal": f"Mindestens {min_figures} client-faehige Abbildungen einbauen: Produkt/Serie, Standort/Unternehmen, Prozess/Zielbild, Architektur/Workflow und Umsetzung/Kosten- oder Nutzenlogik.",
            }
        )
    if profile["tables"] < min_tables:
        issues.append(
            {
                "lint_id": "PROJECT-FIT-TABLE-DENSITY",
                "reason": f"Nur {profile['tables']} native Word-Tabelle(n); erwartet sind mindestens {min_tables}.",
                "goal": "Mindestens Gesellschafts-/Unternehmenstabelle und Projektumfang/Kosten-Tabelle als native Word-Tabellen erzeugen.",
            }
        )
    if not any("kosten" in t.lower() and ("kostenbl" in t.lower() or "liqid" in t.lower() or "service" in t.lower()) for t in profile["table_texts"]):
        issues.append(
            {
                "lint_id": "PROJECT-FIT-MISSING-COST-BREAKDOWN-TABLE",
                "reason": "Keine native Tabelle mit Kostenbloecken bzw. Kostenaufschluesselung erkannt.",
                "goal": "Projektkosten nicht nur im Fliesstext nennen; die Tabelle muss Kostenbloecke/Einzelpositionen sichtbar enthalten.",
            }
        )
    for bad in [
        "für das fördervorhaben ist",
        "fuer das foerdervorhaben ist",
        "die antragstellerin ist",
        "plausibilität",
        "plausibilitaet",
        "recherche zeigt",
        "quellen zeigen",
    ]:
        if bad in text_lower:
            issues.append(
                {
                    "lint_id": "PROJECT-FIT-APP-STYLE-VOICE",
                    "reason": f"App-/Vorlagenwidrige Gutachter- oder Erklaerprosa erkannt: '{bad}'.",
                    "goal": "In Unternehmens-/Dossierperspektive formulieren, nicht als Prueferbericht ueber eine Antragstellerin.",
                }
            )
    first_person_hits = len(re.findall(r"\b(wir|uns|unser(?:e|er|en|em|es)?)\b", text_lower))
    if first_person_hits < 2:
        issues.append(
            {
                "lint_id": "PROJECT-FIT-WEAK-WIR-PERSPECTIVE",
                "reason": "Fast keine Wir-/Unternehmensperspektive erkannt.",
                "goal": "Wenn fachlich passend, Unternehmensbeschreibung in ruhiger Wir-Perspektive schreiben; sonst zumindest direkte interne Feststellungsperspektive ohne Gutachterdistanz.",
            }
        )
    if "projektkosten" not in text_lower or "umsetzungszeitraum" not in text_lower:
        issues.append(
            {
                "lint_id": "PROJECT-FIT-MISSING-COST-TIME-ARCHETYPE",
                "reason": "Projektkosten und Umsetzungszeitraum sind nicht beide als Referenz-Archetyp erkennbar.",
                "goal": "Kosten, Finanzierung/Umfinanzierung und Umsetzungszeitraum nach Vorlagenlogik sauber ausweisen.",
            }
        )
    payload = {
        "ready_to_finish": not issues,
        "needs_revision": bool(issues),
        "issues": issues,
        "metrics": {
            "chars": profile["chars"],
            "tables": profile["tables"],
            "images": profile["images"],
            "comments": profile["comments"],
            "first_person_hits": first_person_hits,
            "reference_docs": len(ref_profiles),
            "reference_images_max": max([r["images"] for r in ref_profiles], default=0),
            "reference_images_avg": round(sum(r["images"] for r in ref_profiles) / max(1, len(ref_profiles)), 1),
        },
        "reference_profiles": ref_profiles[:20],
    }
    return payload


def lint_run(ctx: Ctx, out: str | None = None) -> None:
    run_row(ctx)
    blocks = committed_blocks(ctx)
    text = "\n\n".join(b.get("markdown") or "" for b in blocks)
    issues = lint_text(text)
    payload = {
        "ready_to_finish": not issues,
        "needs_revision": bool(issues),
        "issues": issues,
        "summary": "Foerdervorhaben-App-Baseline-Lint OK." if not issues else f"{len(issues)} App-Baseline-Lint issue(s).",
    }
    emit_json(payload, out)
    record_stage(ctx, "lint_run", {"ready_to_finish": payload["ready_to_finish"], "issues": issues})
    if issues:
        raise SystemExit(2)


def main() -> None:
    parser = argparse.ArgumentParser(description="CLI port of the Foerdervorhaben-Agent app workflow")
    parser.add_argument("--root", default=None, help="CTOX workspace root")
    sub = parser.add_subparsers(dest="cmd", required=True)

    def add_run(p: argparse.ArgumentParser) -> None:
        p.add_argument("--run-id", required=True)

    p = sub.add_parser("init")
    add_run(p)
    p.add_argument("--out")

    p = sub.add_parser("workspace-snapshot")
    add_run(p)
    p.add_argument("--out")

    p = sub.add_parser("asset-lookup")
    add_run(p)
    p.add_argument("--instance-id", action="append", default=[])
    p.add_argument("--out")

    for name, mode in [("writer-prompt", "write"), ("revision-prompt", "revision"), ("flow-review-prompt", "flow_review")]:
        p = sub.add_parser(name)
        add_run(p)
        p.add_argument("--instance-id", action="append", required=True)
        p.add_argument("--out")
        p.set_defaults(mode=mode)

    p = sub.add_parser("lint-text")
    p.add_argument("--file", required=True)
    p.add_argument("--out")

    p = sub.add_parser("lint-docx")
    p.add_argument("--docx", required=True)
    p.add_argument("--run-id")
    p.add_argument("--reference-dir", action="append", default=[])
    p.add_argument("--min-figures", type=int, default=REFERENCE_FIT_DEFAULT_MIN_FIGURES)
    p.add_argument("--min-tables", type=int, default=REFERENCE_FIT_DEFAULT_MIN_TABLES)
    p.add_argument("--skip-reference-fit", action="store_true")
    p.add_argument("--out")

    p = sub.add_parser("reference-fit")
    p.add_argument("--docx", required=True)
    p.add_argument("--reference-dir", action="append", default=[])
    p.add_argument("--min-figures", type=int, default=REFERENCE_FIT_DEFAULT_MIN_FIGURES)
    p.add_argument("--min-tables", type=int, default=REFERENCE_FIT_DEFAULT_MIN_TABLES)
    p.add_argument("--out")

    p = sub.add_parser("lint-run")
    add_run(p)
    p.add_argument("--out")

    args = parser.parse_args()
    root = Path(args.root).resolve() if args.root else repo_root_from(Path.cwd())
    ctx = Ctx(root=root, run_id=getattr(args, "run_id", ""))

    if args.cmd == "init":
        write_init(ctx, args.out)
    elif args.cmd == "workspace-snapshot":
        emit_json(make_snapshot(ctx), args.out)
        record_stage(ctx, "workspace_snapshot", {"out": args.out})
    elif args.cmd == "asset-lookup":
        asset_lookup(ctx, args.instance_id, args.out)
    elif args.cmd in {"writer-prompt", "revision-prompt", "flow-review-prompt"}:
        write_prompt(ctx, args.instance_id, args.mode, args.out)
    elif args.cmd == "lint-text":
        text = Path(args.file).read_text(encoding="utf-8")
        issues = lint_text(text)
        emit_json({"ready_to_finish": not issues, "needs_revision": bool(issues), "issues": issues}, args.out)
        if issues:
            raise SystemExit(2)
    elif args.cmd == "lint-docx":
        ctx = Ctx(root=root, run_id=args.run_id or "")
        issues = lint_text(docx_text(Path(args.docx)))
        fit_payload = None
        if not args.skip_reference_fit:
            fit_payload = project_reference_fit(
                Path(args.docx),
                reference_dirs(root, args.reference_dir),
                args.min_figures,
                args.min_tables,
            )
            issues.extend(fit_payload["issues"])
        payload = {
            "ready_to_finish": not issues,
            "needs_revision": bool(issues),
            "issues": issues,
            "reference_fit": fit_payload,
        }
        emit_json(payload, args.out)
        if ctx.run_id:
            record_stage(ctx, "lint_docx", {"docx": args.docx, "ready_to_finish": payload["ready_to_finish"], "issues": issues})
        if issues:
            raise SystemExit(2)
    elif args.cmd == "reference-fit":
        payload = project_reference_fit(
            Path(args.docx),
            reference_dirs(root, args.reference_dir),
            args.min_figures,
            args.min_tables,
        )
        emit_json(payload, args.out)
        if payload["issues"]:
            raise SystemExit(2)
    elif args.cmd == "lint-run":
        lint_run(ctx, args.out)


if __name__ == "__main__":
    main()
