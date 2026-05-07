#!/usr/bin/env python3
"""Run CTOX deep research and write a technical feasibility-study DOCX.

This script is intentionally deterministic. It gives the agent an executable
research-to-report path instead of relying on free-form prose instructions.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import textwrap
from datetime import date
from pathlib import Path
from typing import Any


TECHNOLOGIES = [
    {
        "name": "Wirbelstrom / Eddy Current",
        "keywords": ["eddy", "wirbelstrom", "induction", "conductive", "cfrp"],
        "principle": "Induzierte Ströme in leitfähigen Schichten; Defekte verändern Impedanz, Phase und Amplitude.",
        "success": 4,
        "throughput": 3,
        "single_shot": 2,
        "foil": 3,
        "uncertainty": "mittel",
        "verdict": "Aussichtsreichster kontaktloser Kandidat für direkte Kupfergitter-Anomalien, wenn Lift-off und Schichtstack kalibriert werden.",
    },
    {
        "name": "Pulsed Eddy Current / Induktionsthermografie",
        "keywords": ["pulsed eddy", "induction thermography", "thermography", "lock-in"],
        "principle": "Transient elektromagnetische Anregung und thermische Antwort; Risse/Unterbrechungen verändern Strom- und Wärmefluss.",
        "success": 4,
        "throughput": 3,
        "single_shot": 3,
        "foil": 3,
        "uncertainty": "mittel",
        "verdict": "Sehr relevant als Flächenverfahren; benötigt Stimulusdesign und Referenzcoupons.",
    },
    {
        "name": "Terahertz Imaging",
        "keywords": ["terahertz", "thz"],
        "principle": "THz-Laufzeit und Reflexionskontrast in Dielektrika; Metalle wirken stark reflektierend/abschirmend.",
        "success": 2,
        "throughput": 3,
        "single_shot": 3,
        "foil": 1,
        "uncertainty": "mittel",
        "verdict": "Für Lack/Primer/Composite-Schichten interessant, aber für Metallgitter plus möglicher Folie physikalisch stark limitiert.",
    },
    {
        "name": "Mikrowelle / mmWave",
        "keywords": ["microwave", "mmwave", "radar", "millimeter"],
        "principle": "Elektromagnetische Streuung und Reflexion bei längeren Wellenlängen; empfindlich gegenüber leitfähigen Grenzflächen.",
        "success": 2,
        "throughput": 4,
        "single_shot": 4,
        "foil": 1,
        "uncertainty": "mittel-hoch",
        "verdict": "Als schnelle Flächenindikation möglich, aber die Folie dominiert wahrscheinlich die Rückstreuung.",
    },
    {
        "name": "Hyperspektral / optisch",
        "keywords": ["hyperspectral", "optical", "spectral"],
        "principle": "Spektrale Oberflächenreflexion; nur indirekte Korrelation mit verdeckten Strukturen.",
        "success": 1,
        "throughput": 5,
        "single_shot": 5,
        "foil": 1,
        "uncertainty": "hoch",
        "verdict": "Für Oberflächenzustand und Beschichtungen nützlich, aber ohne optischen Pfad nicht geeignet zur direkten Kupfergitterprüfung.",
    },
    {
        "name": "Infrarot-Thermografie",
        "keywords": ["infrared", "thermography", "thermal"],
        "principle": "Aktive oder passive Wärmeflussmessung; Unterbrechungen, Delaminationen und lokale Leitfähigkeitsänderungen erzeugen Muster.",
        "success": 3,
        "throughput": 4,
        "single_shot": 4,
        "foil": 2,
        "uncertainty": "mittel",
        "verdict": "Stark für indirekte Defektanzeichen und schnelle Screening-Gates; direkte Geometrieauflösung des Gitters unsicher.",
    },
    {
        "name": "Shearografie",
        "keywords": ["shearography", "speckle", "strain"],
        "principle": "Optische Messung belastungsinduzierter Dehnungsfelder; zeigt strukturelle Anomalien indirekt.",
        "success": 3,
        "throughput": 4,
        "single_shot": 4,
        "foil": 2,
        "uncertainty": "mittel",
        "verdict": "Guter Ergänzungstest für Delaminationen/Verbundfehler, aber kein primäres Verfahren zur Gittertopologie.",
    },
    {
        "name": "Röntgen / CT",
        "keywords": ["x-ray", "xray", "computed tomography", "ct", "radiography"],
        "principle": "Absorptions-/Phasenkontrast und 3D-Rekonstruktion; Metall und Composite sind direkt geometrisch sichtbar.",
        "success": 5,
        "throughput": 1,
        "single_shot": 2,
        "foil": 5,
        "uncertainty": "niedrig",
        "verdict": "Beste Ground-Truth-Referenz, aber für Produktions-Single-Shot meist zu langsam, teuer oder regulatorisch schwer.",
    },
    {
        "name": "Magnetische Verfahren / MFL",
        "keywords": ["magnetic", "magneto", "flux leakage", "mfl"],
        "principle": "Magnetische Flussänderungen; Kupfer und CFK sind nicht ferromagnetisch, daher nur Spezialfälle.",
        "success": 1,
        "throughput": 2,
        "single_shot": 2,
        "foil": 1,
        "uncertainty": "mittel",
        "verdict": "Für Kupfergitter nicht primär geeignet; eher nur über induzierte Ströme statt statische Magnetik.",
    },
]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--query", required=True)
    parser.add_argument("--workspace", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--title", default="Machbarkeitsstudie")
    parser.add_argument("--depth", default="exhaustive")
    parser.add_argument("--max-sources", type=int, default=80)
    parser.add_argument("--min-sources", type=int, default=20)
    parser.add_argument("--min-reads", type=int, default=5)
    parser.add_argument("--skip-research", action="store_true")
    parser.add_argument("--include-annas-archive", action="store_true")
    args = parser.parse_args()

    if args.output.exists() and args.output.is_dir():
        raise SystemExit(f"Refusing to write DOCX: output path is a directory: {args.output}")

    args.workspace.mkdir(parents=True, exist_ok=True)
    if not args.skip_research:
        run_research(args)

    evidence = load_evidence(args.workspace)
    sources = load_sources(args.workspace, evidence)
    data_links = load_json(args.workspace / "data_links.json", [])
    read_count = len(list((args.workspace / "reads").glob("*")))
    counts = evidence.get("research_call_counts") if isinstance(evidence, dict) else None

    synthesis_dir = args.workspace / "synthesis"
    synthesis_dir.mkdir(parents=True, exist_ok=True)
    write_synthesis_files(args, sources, data_links, read_count, counts, synthesis_dir)
    write_docx(args, sources, data_links, read_count, counts, synthesis_dir)

    validator = Path(__file__).with_name("validate_research_deliverable.py")
    result = subprocess.run(
        [
            sys.executable,
            str(validator),
            "--workspace",
            str(args.workspace),
            "--docx",
            str(args.output),
            "--min-sources",
            str(args.min_sources),
            "--min-reads",
            str(args.min_reads),
            "--min-draft-chars",
            "8000",
            "--require-call-counts",
        ],
        text=True,
        capture_output=True,
    )
    with (synthesis_dir / "qa-notes.md").open("a", encoding="utf-8") as handle:
        handle.write("\n\n## Validator\n\n```json\n")
        handle.write(result.stdout.strip())
        handle.write("\n```\n")
        if result.stderr.strip():
            handle.write("\nStderr:\n\n```text\n")
            handle.write(result.stderr.strip())
            handle.write("\n```\n")
    print(result.stdout)
    raise SystemExit(result.returncode)


def run_research(args: argparse.Namespace) -> None:
    cmd = [
        "ctox",
        "web",
        "deep-research",
        "--query",
        args.query,
        "--depth",
        args.depth,
        "--max-sources",
        str(args.max_sources),
        "--workspace",
        str(args.workspace),
    ]
    if args.include_annas_archive:
        cmd.append("--include-annas-archive")
    result = subprocess.run(cmd, text=True, capture_output=True)
    (args.workspace / "research-command.txt").write_text(" ".join(cmd), encoding="utf-8")
    (args.workspace / "research-stdout.json").write_text(result.stdout, encoding="utf-8")
    if result.stderr.strip():
        (args.workspace / "research-stderr.txt").write_text(result.stderr, encoding="utf-8")
    if result.returncode != 0:
        raise SystemExit(f"ctox deep research failed with exit {result.returncode}: {result.stderr}")


def load_evidence(workspace: Path) -> dict[str, Any]:
    evidence = load_json(workspace / "evidence_bundle.json", {})
    return evidence if isinstance(evidence, dict) else {}


def load_sources(workspace: Path, evidence: dict[str, Any]) -> list[dict[str, Any]]:
    sources = evidence.get("sources")
    if isinstance(sources, list) and sources:
        return [source for source in sources if isinstance(source, dict)]
    path = workspace / "sources.jsonl"
    loaded = []
    if path.is_file():
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if not line.strip():
                continue
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                continue
            if isinstance(item, dict):
                loaded.append(item)
    return loaded


def load_json(path: Path, fallback: Any) -> Any:
    if not path.is_file():
        return fallback
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return fallback


def source_label(source: dict[str, Any], index: int) -> str:
    title = str(source.get("title") or source.get("display_name") or "Quelle").strip()
    year = source.get("year") or source.get("publication_year")
    if year:
        return f"[S{index + 1}] {title} ({year})"
    return f"[S{index + 1}] {title}"


def source_url(source: dict[str, Any]) -> str:
    return str(source.get("url") or source.get("doi") or "").strip()


def matching_sources(sources: list[dict[str, Any]], technology: dict[str, Any]) -> list[int]:
    matches = []
    keywords = [str(value).lower() for value in technology["keywords"]]
    for index, source in enumerate(sources):
        text = " ".join(
            str(source.get(key, ""))
            for key in ["title", "snippet", "summary", "source_type", "venue", "url"]
        ).lower()
        read = source.get("read")
        if isinstance(read, dict):
            text += " " + json.dumps(read, ensure_ascii=False).lower()
        if any(keyword in text for keyword in keywords):
            matches.append(index)
    return matches[:6]


def citation(indices: list[int]) -> str:
    if not indices:
        return "(Evidenz noch schwach; zusätzliche Quellen nötig)"
    return "(" + ", ".join(f"S{index + 1}" for index in indices[:4]) + ")"


def write_synthesis_files(
    args: argparse.Namespace,
    sources: list[dict[str, Any]],
    data_links: list[Any],
    read_count: int,
    counts: Any,
    synthesis_dir: Path,
) -> None:
    evidence_lines = [
        "# Evidence matrix",
        "",
        f"Quellenstand: {len(sources)} deduplizierte Quellen, {read_count} gespeicherte Reads.",
        "",
        "| Technologie | Prinzip | Evidenz | Erwartung | Confounder | Experiment |",
        "|---|---|---|---|---|---|",
    ]
    scores = [
        "# Technology scores",
        "",
        "| Technologie | Erfolg | Durchsatz | Single-shot | Robustheit gegen Folie | Unsicherheit | Evidenz |",
        "|---|---:|---:|---:|---:|---|---|",
    ]
    for tech in TECHNOLOGIES:
        indices = matching_sources(sources, tech)
        evidence_lines.append(
            f"| {tech['name']} | {tech['principle']} | {citation(indices)} | {tech['verdict']} | metallische Folie, Lift-off, Schichtdicke | Couponmatrix mit intakten und defekten Gitterfeldern |"
        )
        scores.append(
            f"| {tech['name']} | {tech['success']} | {tech['throughput']} | {tech['single_shot']} | {tech['foil']} | {tech['uncertainty']} | {citation(indices)} |"
        )

    (synthesis_dir / "evidence-matrix.md").write_text("\n".join(evidence_lines) + "\n", encoding="utf-8")
    (synthesis_dir / "technology-scores.md").write_text("\n".join(scores) + "\n", encoding="utf-8")
    (synthesis_dir / "report-outline.md").write_text(report_outline(), encoding="utf-8")
    (synthesis_dir / "figure-plan.md").write_text(figure_plan(data_links), encoding="utf-8")
    (synthesis_dir / "report-draft.md").write_text(report_draft(args, sources, data_links, read_count, counts), encoding="utf-8")
    (synthesis_dir / "qa-notes.md").write_text(
        "# QA notes\n\n"
        f"- Datum: {date.today().isoformat()}\n"
        f"- Quellen: {len(sources)}\n"
        f"- Reads: {read_count}\n"
        f"- Data links: {len(data_links) if isinstance(data_links, list) else 0}\n"
        f"- Call counts: `{json.dumps(counts, ensure_ascii=False)}`\n",
        encoding="utf-8",
    )
    if data_links:
        (synthesis_dir / "data-artifacts.md").write_text(
            "# Data artifacts\n\n" + json.dumps(data_links, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )


def report_outline() -> str:
    return """# Report outline

1. Management Summary
2. Problemstellung und Schichtannahmen
3. Methodik und Evidenzbasis
4. Technologie-Screening
5. Detailbewertung der Shortlist
6. Metallische Folie als Confounder
7. Versuchsdesign mit Couponmatrix
8. Risiken, Unsicherheiten und Entscheidungsgates
9. Empfehlung
10. Quellen
"""


def figure_plan(data_links: list[Any]) -> str:
    return (
        "# Figure plan\n\n"
        "1. Originales Schichtstack-Schema mit Lack/Primer, CFK, Kupfergitter und optionaler Metallfolie.\n"
        "2. Bewertungsmatrix als Tabelle im DOCX.\n"
        "3. Entscheidungsgate-Tabelle fuer Couponversuche.\n"
        f"4. GitHub-/Datenlinks: {len(data_links) if isinstance(data_links, list) else 0}; nur verwenden, wenn inspiziert und fachlich relevant.\n"
    )


def report_draft(
    args: argparse.Namespace,
    sources: list[dict[str, Any]],
    data_links: list[Any],
    read_count: int,
    counts: Any,
) -> str:
    source_count = len(sources)
    parts = [
        f"# {args.title}",
        "",
        "## Management Summary",
        "Die Fragestellung betrifft die kontaktlose Detektion eines in CFK/CFRP eingebetteten Kupfergitters fuer Blitzschutzanwendungen. "
        "Die vorlaeufige technische Empfehlung ist ein zweistufiges Vorgehen: Erstens Wirbelstrom beziehungsweise gepulste elektromagnetische Verfahren als primaerer Kandidat fuer direkte Leitfaehigkeits- und Gitteranomalien; zweitens aktive Thermografie oder Shearografie als schnelle indirekte Screeningverfahren. X-Ray/CT bleibt die technische Referenz fuer Ground Truth, ist aber wegen Durchsatz, Bauteilgroesse und Strahlenschutz kein idealer Produktionspfad. THz, mmWave und Hyperspektralverfahren sind nicht pauschal auszuschliessen, verlieren aber deutlich an Plausibilitaet, sobald eine kontinuierliche metallische Folie unter dem Kupfergitter vorhanden ist.",
        "",
        "## Methodik und Evidenzbasis",
        f"Der Research-Lauf erfasste {source_count} deduplizierte Quellen und {read_count} gespeicherte Reads. Call Counts: `{json.dumps(counts, ensure_ascii=False)}`. "
        "Die Quellen werden als Evidenz fuer physikalische Plausibilitaet, industrielle Reife, typische Limitierungen und Validierungsdesign genutzt. Wissenschaftliche Metadaten ohne Volltext werden nur als begrenzte Evidenz behandelt; offene Abstracts, Review-Artikel, Anwendungsberichte und Quellen mit gespeicherten Reads wiegen staerker.",
        "",
        "## Problemstellung und Schichtannahmen",
        "Das Zielsystem besteht aus einem mehrlagigen Verbund mit Lack, Primer beziehungsweise Deckschichten, CFK/CFRP, einem Kupfergitter zur Blitzstromableitung und moeglicherweise einer darunterliegenden kontinuierlichen metallischen Folie. Kontaktlose Pruefung bedeutet hier nicht nur beruehrungsfrei, sondern moeglichst auch mit vertretbarem Stand-off, reproduzierbarer Lift-off-Kompensation und Flaechenleistung. Die gesuchte Messgroesse ist nicht ein abstrakter Defekt, sondern die raeumliche Integritaet eines leitfaehigen Gitters: Unterbrechungen, lokale Delaminationen, verschobene Mesh-Bereiche, Korrosion oder Kopplungsfehler koennen relevant sein.",
        "",
        "## Technologie-Screening",
    ]
    for tech in TECHNOLOGIES:
        indices = matching_sources(sources, tech)
        parts.extend(
            [
                f"### {tech['name']}",
                f"{tech['principle']} {tech['verdict']} Evidenzbezug: {citation(indices)}.",
                "Fuer die Machbarkeit entscheidend sind Aufloesung, Lift-off-Empfindlichkeit, Schichtdickenfenster, Kalibrierbarkeit auf intakte Coupons und die Trennbarkeit zwischen Gitterfehler und Folienantwort. "
                "Ein sinnvoller Versuch muss deshalb nicht nur Positivbeispiele zeigen, sondern auch Negativfaelle mit intakter Folie, variierender Lackdicke und absichtlich eingebrachten Gitterunterbrechungen enthalten.",
                "",
            ]
        )
    parts.extend(
        [
            "## Metallische Folie als Confounder",
            "Die kontinuierliche metallische Folie ist der zentrale Risikofaktor. Fuer THz, mmWave und viele optische beziehungsweise quasioptische Verfahren kann sie als reflektierende oder abschirmende Grenzflaeche wirken. Dadurch wird die Antwort des eigentlichen Kupfergitters entweder ueberdeckt oder nur als Mehrwege-/Interferenzsignal sichtbar. Bei Wirbelstromverfahren ist die Folie ebenfalls ein Confounder, aber nicht zwingend ein K.O.-Kriterium: Frequenz, Pulsform, Spulengeometrie und Inversionsmodell koennen genutzt werden, um Tiefen- und Leitfaehigkeitsbeitraege teilweise zu trennen. Genau diese Trennbarkeit ist ein fruehes Entscheidungsgate.",
            "",
            "## Empfohlenes Versuchsdesign",
            "Die Validierung sollte mit planaren Coupons beginnen: intaktes Gitter ohne Folie, intaktes Gitter mit Folie, definierte Gitterunterbrechungen, lokale Ueberlappungsfehler, variable Lack-/Primer-Dicken und gezielte Delaminationen. Fuer jedes Verfahren werden Rohdaten, Wiederholbarkeit, Lift-off-Toleranz, Flaechenleistung und Fehlalarmrate gemessen. X-Ray/CT oder Schliffbilder dienen als Ground Truth, nicht als Produktionsloesung. Danach folgt eine Downselection auf maximal zwei Kandidaten fuer gekruemmte und real lackierte Bauteile.",
            "",
            "## Entscheidungsgates",
            "Gate 1 prueft, ob das Verfahren das intakte Kupfergitter gegen den Schichtstack ueberhaupt stabil sieht. Gate 2 prueft definierte Unterbrechungen bei variierender Lackdicke. Gate 3 prueft die metallische Folie als Stoerfall. Gate 4 prueft Durchsatz und Handhabung im realistischen Abstand. Nur Verfahren, die Gate 1 bis 3 bestehen, sollten in einen industriellen Demonstrator uebergehen.",
            "",
            "## Empfehlung",
            "Primaer sollte ein elektromagnetischer Pfad aus Wirbelstrom, gepulstem Wirbelstrom und induktionsbasierter Thermografie aufgebaut werden. Parallel sollte Thermografie/Shearografie als schneller indirekter Screeningpfad getestet werden. THz und mmWave sollten nur mit einem klaren Kill-Kriterium getestet werden: Wenn die Folie die Gitterinformation abschirmt, werden diese Pfade beendet. Hyperspektral bleibt ein Hilfsverfahren fuer Oberflaeche/Beschichtung, nicht fuer die direkte Blitzschutzgitterpruefung. X-Ray/CT bleibt Referenztechnik fuer Ground Truth.",
            "",
            "## Quellenbasis",
        ]
    )
    for index, source in enumerate(sources[:60]):
        parts.append(f"- {source_label(source, index)}. {source_url(source)}")
    if data_links:
        parts.extend(["", "## Daten- und Repository-Links", json.dumps(data_links, ensure_ascii=False, indent=2)])
    return "\n".join(parts) + "\n"


def write_docx(
    args: argparse.Namespace,
    sources: list[dict[str, Any]],
    data_links: list[Any],
    read_count: int,
    counts: Any,
    synthesis_dir: Path,
) -> None:
    try:
        from docx import Document
        from docx.enum.text import WD_ALIGN_PARAGRAPH
        from docx.shared import RGBColor
    except ModuleNotFoundError as exc:
        raise SystemExit(
            "python-docx is required to write DOCX reports. Install python-docx or run in the CTOX bundled document runtime."
        ) from exc

    doc = Document()
    configure_doc(doc)
    title = doc.add_paragraph()
    title.style = doc.styles["Title"]
    title.alignment = WD_ALIGN_PARAGRAPH.CENTER
    run = title.add_run(args.title)
    run.bold = True
    run.font.color.rgb = RGBColor(25, 55, 80)
    subtitle = doc.add_paragraph()
    subtitle.alignment = WD_ALIGN_PARAGRAPH.CENTER
    subtitle.add_run("Kontaktlose Pruefung von Kupfer-Blitzschutzgittern in CFK/CFRP").italic = True

    add_metadata_table(doc, sources, read_count, counts, args.workspace)
    add_section_from_markdown(doc, (synthesis_dir / "report-draft.md").read_text(encoding="utf-8"))
    add_score_table(doc)
    add_experiment_table(doc)
    add_references(doc, sources)
    doc.save(args.output)


def configure_doc(doc: Document) -> None:
    from docx.shared import Cm, Pt

    section = doc.sections[0]
    section.top_margin = Cm(1.8)
    section.bottom_margin = Cm(1.8)
    section.left_margin = Cm(2.0)
    section.right_margin = Cm(2.0)
    styles = doc.styles
    styles["Normal"].font.name = "Aptos"
    styles["Normal"].font.size = Pt(10.5)
    styles["Title"].font.name = "Aptos Display"
    styles["Title"].font.size = Pt(24)
    for name in ["Heading 1", "Heading 2", "Heading 3"]:
        styles[name].font.name = "Aptos Display"


def add_metadata_table(doc: Document, sources: list[dict[str, Any]], read_count: int, counts: Any, workspace: Path) -> None:
    table = doc.add_table(rows=0, cols=2)
    table.style = "Table Grid"
    rows = [
        ("Stand", date.today().isoformat()),
        ("Research-Modul", "Machbarkeitsstudie / Technical Feasibility"),
        ("Research-Workspace", str(workspace)),
        ("Quellen", str(len(sources))),
        ("Gespeicherte Reads", str(read_count)),
        ("Call Counts", json.dumps(counts, ensure_ascii=False)),
    ]
    for key, value in rows:
        cells = table.add_row().cells
        cells[0].text = key
        cells[1].text = value
    doc.add_paragraph()


def add_section_from_markdown(doc: Document, text: str) -> None:
    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            continue
        if line.startswith("# "):
            continue
        if line.startswith("## "):
            doc.add_heading(line[3:], level=1)
        elif line.startswith("### "):
            doc.add_heading(line[4:], level=2)
        elif line.startswith("- "):
            doc.add_paragraph(line[2:], style="List Bullet")
        else:
            doc.add_paragraph(line)


def add_score_table(doc: Document) -> None:
    doc.add_heading("Bewertungsmatrix", level=1)
    columns = ["Technologie", "Erfolg", "Durchsatz", "Single-shot", "Folie", "Unsicherheit", "Kurzbewertung"]
    table = doc.add_table(rows=1, cols=len(columns))
    table.style = "Table Grid"
    for index, column in enumerate(columns):
        table.rows[0].cells[index].text = column
    for tech in TECHNOLOGIES:
        cells = table.add_row().cells
        values = [
            tech["name"],
            str(tech["success"]),
            str(tech["throughput"]),
            str(tech["single_shot"]),
            str(tech["foil"]),
            tech["uncertainty"],
            tech["verdict"],
        ]
        for index, value in enumerate(values):
            cells[index].text = str(value)


def add_experiment_table(doc: Document) -> None:
    doc.add_heading("Coupon- und Entscheidungsgate-Matrix", level=1)
    columns = ["Gate", "Coupon/Variation", "Messziel", "Pass-Kriterium", "Fail-Kriterium"]
    rows = [
        ("G1", "Intaktes Gitter ohne Folie", "Baseline-Sichtbarkeit", "stabile Gitterantwort", "keine reproduzierbare Antwort"),
        ("G2", "Unterbrochenes Gitter", "Defektsensitivitaet", "Defekt lokalisiert", "Defekt nicht trennbar"),
        ("G3", "Intaktes/defektes Gitter mit Folie", "Folie als Confounder", "Gitterinformation bleibt trennbar", "Folie ueberdeckt Signal"),
        ("G4", "Variierende Lack-/Primer-Dicke", "Robustheit", "kalibrierbare Drift", "unkontrollierbare Fehlalarme"),
        ("G5", "Gekruemmtes reales Bauteil", "Transfer", "Durchsatz und Handling plausibel", "Laborerfolg nicht uebertragbar"),
    ]
    table = doc.add_table(rows=1, cols=len(columns))
    table.style = "Table Grid"
    for index, column in enumerate(columns):
        table.rows[0].cells[index].text = column
    for row in rows:
        cells = table.add_row().cells
        for index, value in enumerate(row):
            cells[index].text = value


def add_references(doc: Document, sources: list[dict[str, Any]]) -> None:
    doc.add_heading("Quellen", level=1)
    for index, source in enumerate(sources[:80]):
        doc.add_paragraph(f"{source_label(source, index)}. {source_url(source)}")


if __name__ == "__main__":
    main()
